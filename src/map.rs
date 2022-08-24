use std::{collections::HashMap, net::SocketAddr};

use anyhow::{Context, Result};
use http::{Response, StatusCode};
use serde::Serialize;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use crate::{cmds::Command, guid::Guid, net::PacketData, types::Vector3};

#[derive(Serialize, Debug)]
pub struct PlayerData {
    pub name: String,
    pub pos: Vector3,
}

pub struct MapperCli {
    pub listener_addr: SocketAddr,
    pub from_coord: mpsc::Receiver<Command>,
    pub positions: HashMap<Guid, PlayerData>,
}

impl MapperCli {
    pub async fn handle_commands(mut self) {
        tracing::info!("starting map handler: {}", self.listener_addr);
        let mut listener = TcpListener::bind(self.listener_addr).await.unwrap();
        loop {
            let result = self.handle_command(&mut listener).await;
            if let Err(e) = result {
                tracing::error!("Mapper error: {e:?}");
            }
        }
    }

    pub async fn handle_command(&mut self, listener: &mut TcpListener) -> Result<()> {
        tokio::select! {
            conn = listener.accept() => {
                tracing::info!("Got map connection");
                let (conn, addr) = conn.context("Connection")?;
                let mut conn: BufWriter<TcpStream> = BufWriter::new(conn);
                tracing::info!("Pos: {:?}", &self.positions);
                let send_data: Vec<&PlayerData> = self.positions.values().collect();
                let json_str = serde_json::to_string(&send_data).context("Serializing")?;

                let response = format!("HTTP/1.0 200 OK\r\nContent-Type: application/json\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\n\r\n{}", json_str.len(), json_str);

                let data = response.as_bytes();


                let mut amount = 0;
                let data_len = data.len();
                tracing::info!("Data to write {}", data_len);
                while amount < data_len {

                    let data_written = conn.write(&data[..]).await.unwrap();
                    tracing::info!("Data written: {}", data_written);
                    amount += data_written;
                }
                conn.flush().await.unwrap();
            },
            cmd = self.from_coord.recv() => {
                match cmd {
                    Some(Command::Packet(p)) => {
                        let id = &p.id;
                        match p.data {
                            PacketData::Connect{
                                client_name,
                                ..
                            } => {
                                self.positions.insert(*id, PlayerData{ name: client_name, pos: Vector3::default() });
                            }
                            PacketData::Player{
                                pos,
                                ..
                            } => {
                                self.positions.get_mut(id).map(|x| x.pos = pos);
                            }
                            PacketData::Disconnect{} => {
                                self.positions.remove(id);
                            }
                            _ => {}

                        }
                    }
                    _ => {}
                }
            }
        };
        Ok(())
    }
}
