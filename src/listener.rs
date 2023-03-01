use crate::{
    cmds::{ClientCommand, ServerWideCommand},
    lobby::Lobby,
    types::Result,
};
use enet::{
    host::{self, config::HostConfig, hostevents::HostPollEvent, Host},
    peer::Peer,
};
use std::net::SocketAddr;
use tokio::{net::TcpListener, select, sync::broadcast};

use crate::client::Client;

pub struct Listener {
    pub cli_broadcast: broadcast::Sender<ClientCommand>,
    pub server_broadcast: broadcast::Receiver<ServerWideCommand>,
    pub host: Host,
    pub tcp_bind_addr: SocketAddr,
    pub listener: Option<TcpListener>,
    pub lobby: Lobby,
}

impl Listener {
    pub async fn listen_for_clients(mut self) -> Result<()> {
        let mut host = self.host;

        loop {
            let event = select! {
                conn = host.poll() => {
                    conn.unwrap()
                }
                serv_cmd = self.server_broadcast.recv() => {
                    if let Ok(ServerWideCommand::Shutdown) = serv_cmd {
                        break Ok(())
                    } else {
                        continue
                    }

                }
            };

            let peer: Peer = match event {
                HostPollEvent::NoEvent | HostPollEvent::Disconnect(_) => continue,
                HostPollEvent::Connect(peer) => peer,
            };

            // Fast fail any banned ips before resource allocation
            {
                let settings = self.lobby.settings.read().await;
                let banned_ips = &settings.ban_list.ip_addresses;

                if banned_ips.contains(&peer.get_address().ip()) {
                    tracing::warn!("Banned ip tried to connect: {}", peer.get_address().ip());
                    continue;
                }

                if settings.server.max_players as usize <= self.lobby.players.len() {
                    tracing::warn!("Connection attempt with too many players");
                }
            }

            let to_coord = self.lobby.to_coord.clone();
            let broadcast = self.cli_broadcast.clone();

            tracing::debug!("New client attempting to connect");

            let lobby = self.lobby.clone();
            tokio::spawn(async move {
                let cli_result = Client::initialize_client(peer, to_coord, broadcast, lobby).await;

                if let Err(e) = cli_result {
                    tracing::warn!("Client failed to begin: {}", e)
                }
            });
        }
    }
}
