use crate::{
    cmds::{ClientCommand, ServerWideCommand},
    lobby::Lobby,
    net::connection::Connection,
    types::Result,
};
use std::net::SocketAddr;
use tokio::{net::TcpListener, select, sync::broadcast};

use crate::client::Client;

pub struct Listener {
    pub cli_broadcast: broadcast::Sender<ClientCommand>,
    pub server_broadcast: broadcast::Receiver<ServerWideCommand>,
    pub tcp_bind_addr: SocketAddr,
    pub udp_port_addrs: Option<(u16, u16)>,
    pub listener: Option<TcpListener>,
    pub lobby: Lobby,
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
            socket.set_nodelay(true)?;

            // Fast fail any banned ips before resource allocation
            {
                let settings = self.lobby.settings.read().await;
                let banned_ips = &settings.ban_list.ip_addresses;

                if banned_ips.contains(&addr.ip()) {
                    tracing::warn!("Banned ip tried to connect: {}", addr.to_string());
                    tokio::spawn(async move {
                        Client::ignore_client(Connection::new(socket), addr.to_string()).await
                    });
                    continue;
                }

                if settings.server.max_players as usize <= self.lobby.players.len() {
                    tracing::warn!("Connection attempt with too many players from {}", addr.to_string());
                    tokio::spawn(async move {
                        Client::ignore_client(Connection::new(socket), addr.to_string()).await
                    });
                    continue;
                }
            }

            let to_coord = self.lobby.to_coord.clone();
            let udp_port = udp_port_data.0 + udp_offset;
            let broadcast = self.cli_broadcast.clone();
            udp_offset += 1;
            udp_offset %= udp_port_data.1;

            tracing::debug!("New client attempting to connect");

            let lobby = self.lobby.clone();
            tokio::spawn(async move {
                let cli_result = Client::initialize_client(socket, to_coord, broadcast, udp_port, lobby).await;

                if let Err(e) = cli_result {
                    tracing::warn!("Client failed to begin: {}", e)
                }
            });
        }
    }
}
