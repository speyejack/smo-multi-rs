use anyhow::Result;
use bytes::BytesMut;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use crate::{
    client::{CliRecv, CliSend, Client, SyncClient},
    cmds::{Command, ServerCommand},
    guid::Guid,
    net::Packet,
};

pub struct Server {
    pub to_coord: mpsc::Sender<Command>,
}

impl Server {
    pub async fn listen_for_clients(self) -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:8080").await?;
        loop {
            let (socket, _) = listener.accept().await?;

            let to_coord = self.to_coord.clone();
            tokio::spawn(async move {
                Self::handle_new_client(socket, to_coord).await;

                // let mut buffer = [0; 1024];
                // println!("Connection started");
                // socket.read(&mut buffer).await;
                // time::sleep(Duration::from_secs(3)).await;

                // socket.write_all(b"hello! :)").await;

                println!("Connection happened!");
            });
        }
    }

    async fn handle_new_client(socket: TcpStream, to_coord: mpsc::Sender<Command>) -> Result<()> {
        let (recv, send) = socket.into_split();
        let (to_cli, from_serv) = mpsc::channel(10);
        let cli = Client::default();
        let guid = Guid::default();

        let mut recv = CliRecv {
            guid: guid.clone(),
            socket: recv,
            data: cli.data.clone(),
            to_coord: to_coord.clone(),
            buff: BytesMut::new(),
        };

        let packet = recv.read_packet().await?;
        let result: Result<(), _> = match packet.data {
            crate::net::PacketData::Connect {
                c_type,
                max_player,
                client_name,
            } => todo!(),
            _ => Err(anyhow::anyhow!("First packet was not connect.")),
        };

        let guid = packet.id;
        recv.guid = guid.clone();

        let send = CliSend {
            guid: guid.clone(),
            from_server: from_serv,
            socket: send,
        };

        tokio::spawn(async move { recv.handle_packets().await });
        tokio::spawn(async move { send.handle_packets().await });

        to_coord
            .send(Command::Server(ServerCommand::NewPlayer {
                guid,
                cli,
                comm: to_cli,
            }))
            .await?;

        Ok(())
    }
}
