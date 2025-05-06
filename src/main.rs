mod client;
mod cmds;
mod console;
mod coordinator;
mod guid;
mod json_api;
mod listener;
mod lobby;
mod net;
mod player_holder;
mod server;
mod settings;
mod stages;
mod types;

use crate::types::Result;

use server::Server;
use settings::{load_settings, save_settings};
use tracing_subscriber::filter::{EnvFilter, LevelFilter};
use types::SMOError;

#[tokio::main]
async fn main() -> Result<()> {
    setup_env();
    loop {
        tracing::info!("Creating server");
        let server = create_server();
        tracing::info!("Starting server");
        server.spawn_full_server().await?
    }
}

fn setup_env() {
    // TODO Remove tihs debug panic option
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::builder().with_default_directive(LevelFilter::INFO.into()).from_env_lossy())
        .init();
}

fn create_server() -> Server {
    let settings = load_settings();
    let settings = match settings {
        Ok(s) => s,
        Err(e) => match e {
            SMOError::Io(e) => {
                tracing::warn!("Error opening config file, using default: {e}");
                Default::default()
            }
            SMOError::JsonError(e) => panic!("{e}"),
            _ => Default::default(),
        },
    };

    save_settings(&settings).expect("Failed to save config");

    Server::build_server(settings)
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
        let (to_coord, server, coordinator) = create_server();
        let serv_task = tokio::task::spawn(server.listen_for_clients());
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
