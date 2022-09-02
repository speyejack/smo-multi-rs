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
    pub udp_port: u16,
}

impl Server {
    pub async fn listen_for_clients(self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        let base_udp_port = self.udp_port;
        let mut udp_offset = 0;

        loop {
            let (socket, _) = listener.accept().await?;

            let to_coord = self.to_coord.clone();
            let settings = self.settings.clone();
            let udp_port = base_udp_port + udp_offset;
            udp_offset += 1;
            udp_offset %= 2;

            tracing::info!("New client attempting to connect");

            tokio::spawn(async move {
                let cli_result =
                    Client::initialize_client(socket, to_coord, udp_port, settings).await;

                if let Err(e) = cli_result {
                    tracing::warn!("Client failed to begin: {}", e)
                }
            });
        }
    }
}
