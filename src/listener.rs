use crate::{
    cmds::{ClientCommand, ServerWideCommand},
    types::Result,
};
use enet::{
    host::{self, config::HostConfig, hostevents::HostPollEvent, Host},
    peer::Peer,
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
    pub host: Host,
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
                let settings = self.settings.read().await;
                let banned_ips = &settings.ban_list.ip_addresses;

                if banned_ips.contains(&peer.get_address().ip()) {
                    tracing::warn!("Banned ip tried to connect: {}", peer.get_address().ip());
                    continue;
                }
            }

            let to_coord = self.to_coord.clone();
            let settings = self.settings.clone();
            let broadcast = self.cli_broadcast.clone();

            tracing::debug!("New client attempting to connect");

            tokio::spawn(async move {
                let cli_result =
                    Client::initialize_client(peer, to_coord, broadcast, settings).await;

                if let Err(e) = cli_result {
                    tracing::warn!("Client failed to begin: {}", e)
                }
            });
        }
    }
}
