use crate::{
    cmds::ClientCommand,
    console::Console,
    coordinator::{load_shines, Coordinator, ShineBag},
    json_api::JsonApi,
    listener::Listener,
    lobby::{Lobby, LobbyView},
    settings::Settings,
    types::Result,
};

use std::{net::SocketAddr, sync::Arc};
use tokio::sync::{broadcast, mpsc, RwLock};

pub struct Server {
    pub lobby: Lobby,
    pub cli_broadcast: broadcast::Sender<ClientCommand>,
    pub listener: Listener,
    pub coord: Coordinator,
}

impl Server {
    pub fn build_server(settings: Settings) -> Server {
        let (to_coord, from_clients) = mpsc::channel(100);

        let local_bind_addr = SocketAddr::new(settings.server.address, settings.server.port);

        let _shines = if settings.persist_shines.enabled {
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
        let udp_ports = Some((settings.udp.base_port, settings.udp.port_count));

        let settings = Arc::new(RwLock::new(settings));
        let (cli_broadcast, _) = broadcast::channel(100);

        let (serv_send, serv_recv) = broadcast::channel(1);

        let lobby = Lobby::new(settings, to_coord, serv_send);
        let listener = Listener {
            server_broadcast: serv_recv,
            cli_broadcast: cli_broadcast.clone(),

            tcp_bind_addr: local_bind_addr,
            udp_port_addrs: udp_ports,
            listener: None,
            lobby: lobby.clone(),
        };

        let coord = Coordinator::new(lobby.clone(), from_clients, cli_broadcast.clone());

        Server {
            listener,
            coord,
            cli_broadcast,
            lobby,
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
        let view = LobbyView::new(&self.lobby);
        let console = Console::new(view.clone());
        let json_api = JsonApi::create(view).await?;
        let serv_task = tokio::task::spawn(self.listener.listen_for_clients());
        let coord_task = tokio::task::spawn(self.coord.handle_commands());
        let parser_task = tokio::task::spawn(console.loop_read_commands());
        if let Some(api) = json_api {
            let _api_task = tokio::task::spawn(api.loop_events());
        }

        let _results = tokio::join!(serv_task, coord_task, parser_task);
        Ok(())
    }

    pub fn get_bind_addr(&self) -> SocketAddr {
        self.listener.tcp_bind_addr
    }
}
