use crate::types::Result;
use std::net::SocketAddr;
use tokio::{
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use crate::{client::Client, cmds::Command, settings::SyncSettings};

pub struct Server {
    pub to_coord: mpsc::Sender<Command>,
    pub settings: SyncSettings,
}

impl Server {
    pub async fn listen_for_clients(self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        loop {
            let (socket, _) = listener.accept().await?;

            let to_coord = self.to_coord.clone();
            let settings = self.settings.clone();
            log::info!("New client attempting to connect");

            tokio::spawn(async move {
                let result = Self::handle_new_client(socket, to_coord, settings).await;

                if let Err(e) = result {
                    log::warn!("Client failed to begin")
                }
                // let mut buffer = [0; 1024];
                // println!("Connection started");
                // socket.read(&mut buffer).await;
                // time::sleep(Duration::from_secs(3)).await;

                // socket.write_all(b"hello! :)").await;

                println!("Connection happened!");
            });
        }
    }

    async fn handle_new_client(
        socket: TcpStream,
        to_coord: mpsc::Sender<Command>,
        settings: SyncSettings,
    ) -> Result<()> {
        let cli = Client::initialize_client(socket, to_coord, settings).await?;
        todo!()

        // to_coord
        //     .send(Command::Server(ServerCommand::NewPlayer {
        //         guid,
        //         cli,
        //         comm: to_cli,
        //     }))
        //     .await?;

        // tokio::spawn(async move { cli.handle_events() });

        // Ok(())
    }
}
