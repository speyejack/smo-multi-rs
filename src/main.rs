mod client;
mod cmds;
mod coordinator;
mod guid;
mod map;
mod net;
mod server;
mod settings;
mod types;

use crate::types::Result;

use clap::Parser;
use client::ClientMap;
use cmds::{Cli, Command};
use coordinator::Coordinator;

use map::MapperCli;
use server::Server;
use settings::{Settings, SyncSettings};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufReader, BufWriter, Write},
    net::SocketAddr,
    sync::Arc,
};
use tokio::{
    io::AsyncWriteExt,
    join,
    net::TcpListener,
    sync::{mpsc, RwLock},
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing::info!("Starting server");
    let (_to_coord, server, coordinator, map_cli) = create_default_server();
    let settings = server.settings.read().await;
    let bind_addr = SocketAddr::new(settings.server.address, settings.server.port);
    tracing::info!("Binding tcp port to {}", bind_addr);

    drop(settings);
    let serv_task = tokio::task::spawn(server.listen_for_clients(bind_addr));
    let coord_task = tokio::task::spawn(coordinator.handle_commands());
    let map_task = tokio::task::spawn(map_cli.handle_commands());
    // let parser_task = tokio::task::spawn(parse_commands(to_coord));

    tracing::info!("Server ready");
    // let _results = tokio::join!(serv_task, coord_task, parser_task);
    let _results = tokio::join!(serv_task, coord_task);
    Ok(())
}

fn read_settings() -> Result<Settings> {
    let file = File::open("./settings.json")?;
    let reader = BufReader::new(file);
    let settings = serde_json::from_reader(reader)?;

    Ok(settings)
}

fn save_settings(settings: &Settings) -> Result<()> {
    tracing::debug!("Saving settings");
    let file = File::create("./settings.json")?;
    let writer = BufWriter::new(file);
    serde_json::to_writer_pretty(writer, settings)?;
    Ok(())
}

fn create_default_server() -> (mpsc::Sender<Command>, Server, Coordinator, MapperCli) {
    // TODO Remove tihs debug panic option
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let (to_coord, from_clients) = mpsc::channel(100);
    let (to_map, from_coord) = mpsc::channel(10);

    let settings = read_settings().unwrap_or_default();
    save_settings(&settings).expect("Failed to save config");

    let settings = Arc::new(RwLock::new(settings));

    let server = Server {
        settings: settings.clone(),
        to_coord: to_coord.clone(),
        udp_port: 51888,
    };
    let coordinator = Coordinator {
        shine_bag: Arc::new(RwLock::new(HashSet::default())),
        from_clients,
        settings,
        clients: ClientMap::new(),
        to_clients: HashMap::new(),
        to_mapper: to_map,
    };

    let map_bind_addr = SocketAddr::new("0.0.0.0".parse().unwrap(), 45544_u16.into());
    let map_cli = MapperCli {
        listener_addr: map_bind_addr,
        from_coord,
        positions: HashMap::new(),
    };

    (to_coord, server, coordinator, map_cli)
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
    let command: Cli = join!(task).0?.await?;

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

#[cfg(test)]
mod test {

    use std::{net::SocketAddr, time::Duration};

    use crate::{
        cmds::ServerCommand,
        net::{connection::Connection, Packet},
        types::EncodingError,
    };

    use super::*;

    #[ignore]
    #[tokio::test]
    async fn client_connect() -> Result<()> {
        let addr = "127.0.0.1:61884".parse().unwrap();
        let (to_coord, server, coordinator) = create_default_server();
        let serv_task = tokio::task::spawn(server.listen_for_clients(addr));
        let coord_task = tokio::task::spawn(coordinator.handle_commands());

        let client = tokio::spawn(async move { fake_client(addr).await });

        let _ = tokio::join!(client);
        let cmd = Command::Server(ServerCommand::Shutdown);
        to_coord.send(cmd).await?;
        let _ = tokio::join!(serv_task, coord_task);
        Ok(())
    }

    async fn fake_client(addr: SocketAddr) -> Result<()> {
        let socket = tokio::net::TcpSocket::new_v4()?;
        tracing::debug!("Connecting to server");
        let conn = socket.connect(addr).await?;
        let mut conn = Connection::new(conn);
        tracing::debug!("Connected to server");

        tracing::debug!("Reading data from server");
        let result: Result<Packet> = Err(EncodingError::CustomError.into());
        while result.is_err() {
            let result = conn.read_packet().await;
            tracing::debug!("Packet: {:?}", result);
            // let read = conn.read(&mut buff).await?;
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        tracing::debug!("Read data from server");
        tracing::debug!("Read packet: {:?}", result);
        Ok(())
    }
}
