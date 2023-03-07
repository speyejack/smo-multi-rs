use std::{fmt::Debug, net::SocketAddr, time::Duration};

use crate::{
    guid::Guid,
    net::{connection::Connection, udp_conn::UdpConnection, ConnectionType, Packet, PacketData},
};
use tokio::{
    net::{TcpStream, UdpSocket},
    time::timeout,
};

pub struct MockClient {
    pub guid: Guid,
    pub tcp: Connection,
    pub udp: UdpConnection,
}

impl MockClient {
    pub async fn simple_connect(serv_ip: SocketAddr) -> Self {
        Self::connect(serv_ip, "1000000000-2000-3000-4000-5000000000", "Mock").await
    }

    pub async fn connect<G, S>(serv_ip: SocketAddr, guid: G, name: S) -> Self
    where
        S: Into<String>,
        G: TryInto<Guid>,
        <G as TryInto<Guid>>::Error: Debug,
    {
        let guid = guid.try_into().unwrap();
        tracing::debug!("Starting mock client: {}", guid);
        let tcp_stream = TcpStream::connect(serv_ip)
            .await
            .expect("TCP Stream creation failed");
        let mut tcp = Connection::new(tcp_stream);
        let udp_sock = UdpSocket::bind("127.0.0.1:0")
            .await
            .expect("Couldn't bind udp port");
        let udp = UdpConnection::new(udp_sock, "127.0.0.1".parse().unwrap());

        let init_packet = timeout(Duration::from_millis(100), tcp.read_packet())
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
            client_name: name.into(),
        };

        let connect_packet = Packet::new(guid, data);

        tcp.write_packet(&connect_packet)
            .await
            .expect("Failed to send connect packet");

        Self { guid, tcp, udp }
    }

    pub async fn get_packet(&mut self) -> Packet {
        let packet = tokio::select! {
            packet = self.tcp.read_packet() => packet,
            packet = self.udp.read_packet() => packet,
        };

        if let Err(ref e) = packet {
            tracing::error!("Failed to parse packet: {}", e);
        }
        packet.expect("Failed to parse packet")
    }

    pub async fn send_packet(&mut self, p: &Packet) {
        if self.udp.is_client_udp() {
            match p.data {
                PacketData::Player { .. } | PacketData::Cap { .. } => self
                    .udp
                    .write_packet(p)
                    .await
                    .expect("Failed to send udp packet"),
                _ => self
                    .tcp
                    .write_packet(p)
                    .await
                    .expect("Failed to send tcp packet"),
            }
        } else {
            self.tcp
                .write_packet(p)
                .await
                .expect("Failed to send packet");
        }
    }

    pub async fn replay_player(mut self, target: Guid) {
        tracing::debug!("Beginning player replay");
        loop {
            let mut packet = self.get_packet().await;
            if packet.id != target {
                continue;
            }

            match packet.data {
                PacketData::Player { ref mut pos, .. } | PacketData::Cap { ref mut pos, .. } => {
                    pos.y += 200.0;
                }
                PacketData::Game { .. }
                | PacketData::Tag { .. }
                | PacketData::Capture { .. }
                | PacketData::ChangeStage { .. } => {}
                PacketData::UdpInit { port } => self.udp.set_client_port(port),
                _ => continue,
            }

            let new_packet = Packet::new(self.guid, packet.data);
            self.send_packet(&new_packet).await;
        }
    }
}
