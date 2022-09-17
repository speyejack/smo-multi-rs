mod client;
mod cmds;
mod coordinator;
mod guid;
mod listener;
mod net;
mod server;
mod settings;
mod types;

use crate::types::Result;

use server::Server;
use settings::Settings;
use std::{
    fs::File,
    io::{BufReader, BufWriter},
    net::SocketAddr,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing::info!("Starting server");
    let server = create_server();
    let settings = server.settings.read().await;
    let bind_addr = SocketAddr::new(settings.server.address, settings.server.port);
    tracing::info!("Binding tcp port to {}", bind_addr);

    drop(settings);
    tracing::info!("Server ready");
    server.spawn_minimal_server(bind_addr).await
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

fn create_server() -> Server {
    // TODO Remove tihs debug panic option
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let settings = read_settings().unwrap_or_default();
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
