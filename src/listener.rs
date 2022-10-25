use crate::{
    cmds::{ClientCommand, ServerWideCommand},
    types::Result,
};
use std::net::SocketAddr;
use tokio::{
    net::TcpListener,
    select,
    sync::{broadcast, mpsc},
};

use crate::{client::Client, cmds::Command, settings::SyncSettings};

pub struct Listener {
    pub to_coord: mpsc::Sender<Command>,
    pub cli_broadcast: broadcast::Sender<ClientCommand>,
    pub server_broadcast: broadcast::Receiver<ServerWideCommand>,
    pub settings: SyncSettings,
    pub tcp_bind_addr: SocketAddr,
    pub udp_port_addrs: Option<(u16, u16)>,
    pub listener: Option<TcpListener>,
}

impl Listener {
    pub async fn bind_address(&mut self) -> Result<()> {
        let listener = TcpListener::bind(self.tcp_bind_addr).await?;
        self.tcp_bind_addr = listener.local_addr().unwrap();
        self.listener = Some(listener);
        Ok(())
    }

    pub async fn listen_for_clients(mut self) -> Result<()> {
        if self.listener.is_none() {
            self.bind_address().await?;
        }
        let listener = self.listener.unwrap();
        tracing::info!("Binding tcp port to {}", self.tcp_bind_addr);

        let udp_port_data = self.udp_port_addrs.unwrap_or((0, 1));
        let mut udp_offset = 0;

        loop {
            let (socket, addr) = select! {
                conn = listener.accept() => {
                    conn?
                }
                serv_cmd = self.server_broadcast.recv() => {
                    if let Ok(ServerWideCommand::Shutdown) = serv_cmd {
                        break Ok(())
                    } else {
                        continue
                    }

                }
            };

            // Fast fail any banned ips before resource allocation
            {
                let settings = self.settings.read().await;
                let banned_ips = &settings.ban_list.ip_addresses;

                if banned_ips.contains(&addr.ip()) {
                    tracing::warn!("Banned ip tried to connect: {}", addr.ip());
                    continue;
                }
            }

            let to_coord = self.to_coord.clone();
            let settings = self.settings.clone();
            let udp_port = udp_port_data.0 + udp_offset;
            let broadcast = self.cli_broadcast.clone();
            udp_offset += 1;
            udp_offset %= udp_port_data.1;

            tracing::info!("New client attempting to connect");

            tokio::spawn(async move {
                let cli_result =
                    Client::initialize_client(socket, to_coord, broadcast, udp_port, settings)
                        .await;

                if let Err(e) = cli_result {
                    tracing::warn!("Client failed to begin: {}", e)
                }
            });
        }
    }
}
