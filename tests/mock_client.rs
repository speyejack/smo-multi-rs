use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use smoo::{
    coordinator::Coordinator,
    guid::Guid,
    listener::Listener,
    net::{connection::Connection, udp_conn::UdpConnection, ConnectionType, Packet, PacketData},
    settings::Settings,
};
use tokio::{
    net::TcpStream,
    select,
    sync::{mpsc, RwLock},
    time::timeout,
};
use tracing_subscriber::EnvFilter;

struct MockClient {
    pub guid: Guid,
    pub tcp: Connection,
    pub udp: Option<UdpConnection>,
}

impl MockClient {
    pub async fn connect(serv_ip: SocketAddr) -> Self {
        let guid = Guid::default();
        let tcp_stream = TcpStream::connect(serv_ip)
            .await
            .expect("TCP Stream creation failed");
        let mut tcp = Connection::new(tcp_stream);

        let init_packet = timeout(Duration::from_millis(1), tcp.read_packet())
            .await
            .expect("Init packet timed out")
            .expect("Init packet recv failed");

        match init_packet.data {
            PacketData::Init { max_players } => assert!(max_players > 0),
            _ => panic!("First packet not init packet"),
        }

        let data = PacketData::Connect {
            c_type: ConnectionType::FirstConnection,
            max_player: u16::MAX,
            client_name: "Mock".to_string(),
        };

        let connect_packet = Packet::new(guid, data);

        tcp.write_packet(&connect_packet)
            .await
            .expect("Failed to send connect packet");

        Self {
            guid,
            tcp,
            udp: None,
        }
    }

    pub async fn get_packet(&mut self) -> Packet {
        let packet = self
            .tcp
            .read_packet()
            .await
            .expect("Failed to parse packet");
        packet
    }

    pub async fn send_packet(&mut self, p: &Packet) {
        let packet = self
            .tcp
            .write_packet(&p)
            .await
            .expect("Failed to send packet");
        packet
    }

    pub async fn replay_player(mut self) {
        loop {
            let packet = self.get_packet().await;

            match packet.data {
                PacketData::Player { mut pos, .. } | PacketData::Cap { mut pos, .. } => {
                    pos.y += 1000.0;
                }
                PacketData::Game { .. }
                | PacketData::Tag { .. }
                | PacketData::Capture { .. }
                | PacketData::ChangeStage { .. } => {}
                PacketData::UdpInit { port } => tracing::warn!("Mock didnt handle udp"),
                _ => continue,
            }

            let new_packet = Packet::new(self.guid, packet.data);
            self.send_packet(&new_packet).await;
        }
    }
}

// #[ignore = "Only used testing mock"]
#[tokio::test]
async fn replay_server() {
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

    let serv_task = tokio::task::spawn(server.listen_for_clients("0.0.0.0:33425".parse().unwrap()));
    let coord_task = tokio::task::spawn(coordinator.handle_commands());
    tokio::time::sleep(Duration::from_secs(1)).await;

    let mock_client = MockClient::connect("127.0.0.1:33425".parse().unwrap()).await;
    let cli_task = tokio::task::spawn(mock_client.replay_player());

    let _ = tokio::join!(serv_task, coord_task, cli_task);
}
