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
            tracing::info!("New client attempting to connect");

            tokio::spawn(async move {
                let cli_result = Client::initialize_client(socket, to_coord, settings).await;

                if let Err(e) = cli_result {
                    tracing::warn!("Client failed to begin: {}", e)
                }
            });
        }
    }
}
