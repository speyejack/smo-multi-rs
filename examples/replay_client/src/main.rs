use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
    str::FromStr,
    sync::Arc,
    time::Duration,
};

use smoo::{
    coordinator::Coordinator,
    guid::Guid,
    listener::Listener,
    net::{connection::Connection, udp_conn::UdpConnection, ConnectionType, Packet, PacketData},
    settings::Settings,
    test::mockclient::MockClient,
};
use tokio::{
    net::{TcpStream, UdpSocket},
    select,
    sync::{mpsc, RwLock},
    time::timeout,
};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let default_panic = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        default_panic(info);
        std::process::exit(1);
    }));

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let settings = Arc::new(RwLock::new(Settings::default()));
    let (to_coord, from_clients) = mpsc::channel(100);

    let server = Listener {
        settings: settings.clone(),
        to_coord: to_coord.clone(),
        udp_port: 51888,
    };

    let coordinator = Coordinator {
        shine_bag: Arc::new(RwLock::new(HashSet::default())),
        from_clients,
        settings,
        clients: HashMap::new(),
    };

    let serv_task = tokio::task::spawn(server.listen_for_clients("0.0.0.0:1029".parse().unwrap()));
    let coord_task = tokio::task::spawn(coordinator.handle_commands());
    tokio::time::sleep(Duration::from_secs(1)).await;

    let mock_client = MockClient::connect("127.0.0.1:1029".parse().unwrap()).await;
    let target_guid = [
        126, 128, 87, 52, 186, 45, 0, 16, 175, 237, 95, 234, 197, 104, 21, 75,
    ];
    let cli_task = tokio::task::spawn(mock_client.replay_player(target_guid.into()));

    let _ = tokio::join!(serv_task, coord_task, cli_task);
}
