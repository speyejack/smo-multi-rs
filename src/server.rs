use crate::{
    cmds::ClientCommand,
    console::parse_commands,
    coordinator::{load_shines, Coordinator, ShineBag},
    listener::Listener,
    settings::{Settings, SyncSettings},
    types::Result,
};

use enet::host::{config::HostConfig, Host};
use std::{net::SocketAddr, sync::Arc};
use tokio::{
    net::UdpSocket,
    sync::{broadcast, mpsc, RwLock},
};

use crate::cmds::Command;

pub struct Server {
    pub settings: SyncSettings,
    pub to_coord: mpsc::Sender<Command>,
    pub cli_broadcast: broadcast::Sender<ClientCommand>,
    pub listener: Listener,
    pub coord: Coordinator,
}

impl Server {
    pub fn build_server(settings: Settings, socket: UdpSocket) -> Server {
        let (to_coord, from_clients) = mpsc::channel(100);

        let local_bind_addr = SocketAddr::new(settings.server.address, settings.server.port);

        let shines = if settings.persist_shines.enabled {
            let result = load_shines(&settings.persist_shines.filename);

            match result {
                Ok(bag) => bag,
                Err(e) => {
                    tracing::warn!(
                        "Failed to load shine bag using empty shine bag instead: {}",
                        e
                    );
                    ShineBag::default()
                }
            }
        } else {
            ShineBag::default()
        };
        let max_players = settings.server.max_players;
        let settings = Arc::new(RwLock::new(settings));
        let (cli_broadcast, _) = broadcast::channel(100);

        let (serv_send, serv_recv) = broadcast::channel(1);
        let host = Host::create(HostConfig::new(max_players.into()).unwrap(), socket).unwrap();
        let listener = Listener {
            server_broadcast: serv_recv,
            settings: settings.clone(),
            to_coord: to_coord.clone(),
            cli_broadcast: cli_broadcast.clone(),
            host,
        };

        let coord = Coordinator::new(
            settings.clone(),
            from_clients,
            cli_broadcast.clone(),
            serv_send,
            shines,
        );

        Server {
            settings,
            to_coord,
            listener,
            coord,
            cli_broadcast,
        }
    }

    // pub async fn bind_addresses(&mut self) -> Result<()> {
    //     self.listener.bind_address().await
    // }

    pub async fn spawn_minimal_server(self) -> Result<()> {
        let serv_task = tokio::task::spawn(self.listener.listen_for_clients());
        let coord_task = tokio::task::spawn(self.coord.handle_commands());

        let _result = tokio::join!(serv_task, coord_task);
        Ok(())
    }

    pub async fn spawn_full_server(self) -> Result<()> {
        let rx = self.coord.server_broadcast.subscribe();
        let serv_task = tokio::task::spawn(self.listener.listen_for_clients());
        let coord_task = tokio::task::spawn(self.coord.handle_commands());
        let parser_task = tokio::task::spawn(parse_commands(self.to_coord.clone(), rx));

        let _results = tokio::join!(serv_task, coord_task, parser_task);
        Ok(())
    }

    pub fn get_bind_addr(&self) -> SocketAddr {
        self.listener.host.get_bind_address()
    }
}
