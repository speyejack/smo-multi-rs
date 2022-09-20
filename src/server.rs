use crate::{
    cmds::Cli,
    coordinator::Coordinator,
    listener::Listener,
    settings::{Settings, SyncSettings},
    types::Result,
};
use clap::Parser;
use std::{
    collections::{HashMap, HashSet},
    io::Write,
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
            udp_port: 51888,
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

async fn parse_commands(mut to_coord: mpsc::Sender<Command>) -> Result<()> {
    loop {
        let command_result = parse_command(&mut to_coord).await;

        if let Err(e) = command_result {
            println!("{}", e)
        }
    }
}

async fn parse_command(to_coord: &mut mpsc::Sender<Command>) -> Result<()> {
    let task = tokio::task::spawn_blocking(|| async { read_command() });
    let command: Cli = tokio::join!(task).0?.await?;

    Ok(to_coord.send(Command::Cli(command.cmd)).await?)
}

fn read_command() -> Result<Cli> {
    let mut input = "> ".to_string();

    print!("{}", input);
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().split(' ');
    let cli = Cli::try_parse_from(input)?;
    Ok(cli)
}
