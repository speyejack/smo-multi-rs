use crate::{
    console::parse_commands,
    coordinator::Coordinator,
    listener::Listener,
    settings::{Settings, SyncSettings},
    types::Result,
};

use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
};
use tokio::sync::{mpsc, RwLock};

use crate::cmds::Command;

pub struct Server {
    pub settings: SyncSettings,
    pub to_coord: mpsc::Sender<Command>,
    pub listener: Listener,
    pub coord: Coordinator,
}

impl Server {
    pub fn build_server(settings: Settings) -> Server {
        let (to_coord, from_clients) = mpsc::channel(100);

        let local_bind_addr = SocketAddr::new(settings.server.address, settings.server.port);

        let settings = Arc::new(RwLock::new(settings));

        let listener = Listener {
            settings: settings.clone(),
            to_coord: to_coord.clone(),
            tcp_bind_addr: local_bind_addr,
            udp_port_addrs: Some((51888, 32)),
            listener: None,
        };

        let coord = Coordinator {
            shine_bag: Arc::new(RwLock::new(HashSet::default())),
            from_clients,
            settings: settings.clone(),
            clients: HashMap::new(),
        };

        Server {
            settings,
            to_coord,
            listener,
            coord,
        }
    }

    pub async fn bind_addresses(&mut self) -> Result<()> {
        self.listener.bind_address().await
    }

    pub async fn spawn_minimal_server(self) -> Result<()> {
        let serv_task = tokio::task::spawn(self.listener.listen_for_clients());
        let coord_task = tokio::task::spawn(self.coord.handle_commands());

        let _result = tokio::join!(serv_task, coord_task);
        Ok(())
    }

    pub async fn spawn_full_server(self) -> Result<()> {
        let serv_task = tokio::task::spawn(self.listener.listen_for_clients());
        let coord_task = tokio::task::spawn(self.coord.handle_commands());
        let parser_task = tokio::task::spawn(parse_commands(self.to_coord.clone()));

        let _results = tokio::join!(serv_task, coord_task, parser_task);
        Ok(())
    }

    pub fn get_bind_addr(&self) -> SocketAddr {
        self.listener.tcp_bind_addr
    }
}
