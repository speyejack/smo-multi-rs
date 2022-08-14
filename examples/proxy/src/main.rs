mod udp_conn;

use bytes::BytesMut;
use smoo::net::connection::Connection;
use smoo::net::{encoding::Encodable, Packet, PacketData};
use smoo::types::Result;
use std::ops::Not;
use std::time::Instant;
use std::{io::Cursor, net::SocketAddr};
use tokio;
use tokio::net::UdpSocket;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tracing::Instrument;
use tracing_subscriber::EnvFilter;
use udp_conn::UdpConnection;

type LocalAddrs = (SocketAddr, SocketAddr);
type RemoteAddrs = (SocketAddr, SocketAddr, Origin);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::debug!("Starting");

    let mut args = std::env::args();
    let is_client = args.nth(1).unwrap() == "client";
    let (span, local_bind, remote_addrs) = if is_client {
        // Client side
        tracing::info!("Client side proxy");
        let local_bind: LocalAddrs = (
            "0.0.0.0:1027".parse().unwrap(),  // TCP
            "0.0.0.0:55446".parse().unwrap(), // UDP
        );
        let remote_addrs: RemoteAddrs = (
            "192.168.1.39:1028".parse().unwrap(),
            "192.168.1.39:55445".parse().unwrap(),
            // "127.0.0.1:61885".parse().unwrap(),
            // "127.0.0.1:55445".parse().unwrap(),
            Origin::Server,
        );
        (tracing::info_span!("client"), local_bind, remote_addrs)
    } else {
        // Server side
        tracing::info!("Server side proxy");
        let local_bind: LocalAddrs = (
            "0.0.0.0:61885".parse().unwrap(), // TCP
            "0.0.0.0:55445".parse().unwrap(), // UDP
        );
        let remote_addrs: RemoteAddrs = (
            "64.201.219.20:1027".parse().unwrap(), // Server address
            // "127.0.0.1:61888".parse().unwrap(),
            // "127.0.0.1:55446".parse().unwrap(),
            "192.168.1.39:55446".parse().unwrap(),
            Origin::Client,
        );
        (tracing::info_span!("server"), local_bind, remote_addrs)
    };
    let _span = span.enter();

    let addr: SocketAddr = local_bind.0;
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (from_socket, addr) = listener.accept().await?;
        let udp = UdpSocket::bind(local_bind.1).await.unwrap();
        tracing::info!("new client connection: {}", addr);
        let span = tracing::info_span!("cli", addr = addr.ip().to_string());

        let remote_addrs = remote_addrs.clone();
        tokio::spawn(
            async move {
                let result = proxy_client(from_socket, udp, remote_addrs).await;
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

async fn proxy_client(
    cli_sock: TcpStream,
    udp_sock: UdpSocket,
    to_addrs: RemoteAddrs,
) -> Result<()> {
    let server_conn = tokio::net::TcpSocket::new_v4()?;
    let addr = to_addrs.0;
    let udp_addr = to_addrs.1;
    let plex = to_addrs.2;
    let serv_sock = server_conn.connect(addr).await?;
    // udp_sock.connect(udp_addr).await.unwrap();

    let mut cli = Connection::new(cli_sock);
    let mut serv = Connection::new(serv_sock);
    let mut udp = UdpConnection::new(udp_sock, udp_addr);
    let mut use_udp = true;
    let mut last_tag_packet = Instant::now();

    tracing::info!("Client setup and ready");
    loop {
        let (origin, packet_result) = tokio::select! {
            packet_r = udp.read_packet() => {(plex, packet_r)}
            packet_r = cli.read_packet() => {(Origin::Client, packet_r)},
            packet_r = serv.read_packet() => {(Origin::Server, packet_r)},
        };
        tracing::trace!("packet: {:?}", &packet_result);

        let mut packet = packet_result?;
        packet.resize();

        let (origin_conn, dest_conn) = match origin {
            Origin::Client => (&mut cli, &mut serv),
            Origin::Server => (&mut serv, &mut cli),
        };

        tracing::debug!("got packet: {}", packet.data.get_type_name());
        if let &Packet {
            data: PacketData::Tag { .. },
            ..
        } = &packet
        {
            if last_tag_packet.elapsed().as_millis() < 1000 {
                use_udp = !use_udp;
                tracing::info!("Using udp: {}", use_udp);
            }
            last_tag_packet = Instant::now();
        };

        if use_udp && origin != plex {
            if let &Packet {
                data: PacketData::Player { .. },
                ..
            } = &packet
            {
                udp.write_packet(&packet).await.unwrap();
                continue;
            }
        }
        dest_conn.write_packet(&packet).await?
    }
}
