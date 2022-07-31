use crate::cmds::Command;
use crate::guid::Guid;
use crate::net::{encoding::Decodable, Packet};
use anyhow::Result;
use bytes::{Buf, BytesMut};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::{mpsc, Mutex};

pub type ClientMap = HashMap<Guid, SyncClient>;
pub type SyncClient = Arc<Mutex<ClientData>>;

#[derive(Default, Clone, Debug)]
pub struct Client {
    pub data: SyncClient,
}

#[derive(Default, Clone, Debug)]
pub struct ClientData {
    pub shine_sync: HashSet<i32>,
    pub scenario: u8,
    pub is_2d: bool,
    pub last_game_packet: Option<Packet>,
    pub speedrun: bool,
    pub loaded_save: bool,
    pub time: Duration,
}

pub struct CliRecv {
    pub guid: Guid,
    pub socket: OwnedReadHalf,
    pub data: SyncClient,
    pub to_coord: mpsc::Sender<Command>,
    pub buff: bytes::BytesMut,
    pub alive: bool,
}

impl CliRecv {
    pub async fn handle_packets(mut self) -> Result<()> {
        while self.alive {
            self.handle_packet().await?;
        }
        Ok(())
    }

    async fn handle_packet(&mut self) -> Result<()> {
        let result_packet = self.read_packet().await;
        if let Ok(packet) = result_packet {
            self.to_coord.send(Command::Packet(packet)).await?;
        }
        Ok(())
    }

    pub async fn read_packet(&mut self) -> Result<Packet> {
        let read_amount = self.socket.read(&mut self.buff[..]).await?;
        if read_amount == 0 {
            self.alive = false;
        }
        let result: Packet = Packet::decode(&mut self.buff.split())?;
        self.buff.advance(read_amount);
        Ok(result)
    }
}

pub struct CliSend {
    pub guid: Guid,
    pub from_server: mpsc::Receiver<Packet>,
    pub socket: OwnedWriteHalf,
    pub alive: bool,
}

impl CliSend {
    pub async fn handle_packets(mut self) -> Result<()> {
        while self.alive {
            self.handle_packet().await?;
        }

        self.socket.forget();
    }

    async fn handle_packet(&mut self) -> Result<()> {
        let packet = self.from_server.recv().await;
        if let Some(p) = packet {
            if p.id == self.guid {
                if let crate::net::PacketData::Disconnect = p.data {
                    self.alive = false;
                }
            } else {
                self.send_packet(&p).await?;
            }
        }
        Ok(())
    }

    async fn send_packet(&mut self, packet: &Packet) -> Result<()> {
        let mut buff = BytesMut::with_capacity(300);
        // let mut buffer: [u8; 100] = [0; 100];
        // let mut cursor = Cursor::new(&mut buffer[..]);
        // let size = bincode::serialized_size(&packet).unwrap() as usize;
        // bincode::serialize_into(&mut cursor, &packet)?;
        // let amount = self.socket.write(&buffer[..size]).await?;
        // assert!(amount == size);
        Ok(())
    }
}
