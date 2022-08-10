mod udp_conn;

use bytes::BytesMut;
use smoo::net::connection::Connection;
use smoo::net::{encoding::Encodable, Packet, PacketData};
use smoo::types::Result;
use std::ops::Not;
use std::{io::Cursor, net::SocketAddr};
use tokio;
use tokio::net::UdpSocket;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tracing::Instrument;
use tracing_subscriber::EnvFilter;
use udp_conn::UdpConnection;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::debug!("Starting");
    let addr: SocketAddr = "0.0.0.0:61884".parse().unwrap();
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (from_socket, addr) = listener.accept().await?;
        let udp = UdpSocket::bind("0.0.0.0:55443").await.unwrap();
        let direction = Origin::Client;
        tracing::info!("new client connection: {}", addr);
        let span = tracing::info_span!("cli", addr = addr.ip().to_string());

        tokio::spawn(
            async move {
                let result = proxy_client(from_socket, udp, direction).await;
                if let Err(e) = result {
                    tracing::warn!("Client error: {e}");
                }
            }
            .instrument(span),
        );
    }

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Origin {
    Client,
    Server,
}

impl Not for Origin {
    type Output = Self;
    fn not(self) -> Self::Output {
        match self {
            Origin::Client => Origin::Server,
            Origin::Server => Origin::Client,
        }
    }
}

async fn proxy_client(cli_sock: TcpStream, udp_sock: UdpSocket, plex: Origin) -> Result<()> {
    let server_conn = tokio::net::TcpSocket::new_v4()?;
    let addr = "192.168.1.39:1027".parse().unwrap();
    let serv_sock = server_conn.connect(addr).await?;

    let mut cli = Connection::new(cli_sock);
    let mut serv = Connection::new(serv_sock);
    let mut udp = UdpConnection::new(udp_sock);

    tracing::info!("Client setup and ready");
    loop {
        let (origin, packet_result) = tokio::select! {
            packet_r = udp.read_packet() => {(!plex, packet_r)}
            packet_r = cli.read_packet() => {(Origin::Client, packet_r)},
            packet_r = serv.read_packet() => {(Origin::Server, packet_r)},
        };
        tracing::debug!("packet: {:?}", &packet_result);

        let mut packet = packet_result?;
        packet.resize();

        let (origin_conn, dest_conn) = match origin {
            Origin::Client => (&mut cli, &mut serv),
            Origin::Server => (&mut serv, &mut cli),
        };

        if origin == plex {
            if let &Packet {
                id: _,
                data_size: _,
                data: PacketData::Player { .. },
            } = &packet
            {
                udp.write_packet(&packet).await?;
                continue;
            }
        }

        // let mut buff = BytesMut::with_capacity(300);
        // packet.encode(&mut buff);
        // assert_eq!(&current_conn.last_data[..].len(), &buff[..].len());

        tracing::info!("got packet: {}", packet.data.get_type_name());

        dest_conn.write_packet(&packet).await?
    }
}
