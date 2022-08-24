use bytes::BytesMut;
use smoo::guid::Guid;
use smoo::net::connection::Connection;
use smoo::net::udp_conn::UdpConnection;
use smoo::net::{encoding::Encodable, Packet, PacketData};
use smoo::types::Result;
use std::ops::Not;
use std::time::Instant;
use std::{io::Cursor, net::SocketAddr, net::ToSocketAddrs};
use tokio;
use tokio::net::UdpSocket;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tracing::Instrument;
use tracing_subscriber::EnvFilter;

type LocalAddrs = (SocketAddr, SocketAddr);
type RemoteAddrs = (SocketAddr, SocketAddr, Origin);

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::debug!("Starting");

    let mut args = std::env::args();
    let proxy_type = args.nth(1).unwrap();
    let (span, local_bind, remote_addrs) = if proxy_type == "client" {
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
    } else if proxy_type == "server" {
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
    } else if proxy_type == "proxy" {
        // Client side
        let serv_ip = std::env::args().nth(2).unwrap();
        let local_tcp_ip = std::env::args().nth(3).unwrap();
        let local_udp_ip = std::env::args().nth(4).unwrap();
        let local_bind: LocalAddrs = (
            local_tcp_ip.parse().unwrap(), // TCP
            local_udp_ip.parse().unwrap(), // UDP
        );

        // dns resolve server string, otherwise try to parse it as IPv4
        let mut server_addrs = serv_ip.to_socket_addrs().unwrap();
        let server_addr: SocketAddr = match server_addrs.next() {
            Some(addr) => addr,
            None => serv_ip.parse().unwrap(),
        };

        let remote_addrs: RemoteAddrs = (
            server_addr,
            "127.0.0.1:55445".parse().unwrap(), // Junk address
            Origin::Server,
        );
        tracing::info!("Udp proxy started on {}", local_bind.0);
        (tracing::info_span!("proxy"), local_bind, remote_addrs)
    } else {
        panic!("Invalid first parameter '{}', you probably want 'proxy' followed by 'serv ip:port' 'local_tcp_bind:port' 'local_udp_bind:port'", proxy_type)
    };
    let _span = span.enter();

    let addr: SocketAddr = local_bind.0;
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (from_socket, addr) = listener.accept().await?;
        tracing::info!("new client connection: {}", addr);
        let span = tracing::info_span!("cli", addr = addr.ip().to_string());

        let remote_addrs = remote_addrs.clone();
        tokio::spawn(
            async move {
                let result = proxy_client(from_socket, local_bind.1, remote_addrs).await;
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
    udp_addr_loc: SocketAddr,
    to_addrs: RemoteAddrs,
) -> Result<()> {
    let server_conn = tokio::net::TcpSocket::new_v4()?;
    let addr = to_addrs.0;
    let serv_udp_addr = to_addrs.1;
    let plex = to_addrs.2;
    let serv_sock = server_conn.connect(addr).await?;
    let serv_tcp_addr = serv_sock.peer_addr().expect("Couldn't get tcp peer addr");
    let serv_udp_addr = SocketAddr::new(serv_tcp_addr.ip(), serv_udp_addr.port());

    let udp = UdpSocket::bind(udp_addr_loc).await.unwrap();
    let loc_udp_addr = udp.local_addr().expect("Couldn't get udp local port");
    let loc_udp_port = loc_udp_addr.port();
    tracing::debug!("Binding udp to: {}", loc_udp_addr);

    let mut cli = Connection::new(cli_sock);
    let mut serv = Connection::new(serv_sock);
    let mut udp = UdpConnection::from_connection(udp, serv_udp_addr);
    let mut use_udp = true;
    let mut last_tag_packet = Instant::now();

    tracing::info!("Client setup and ready");
    loop {
        let (origin, packet_result) = tokio::select! {
            packet_r = udp.read_packet() => {tracing::trace!("Got udp!");(plex, packet_r)}
            packet_r = cli.read_packet() => {(Origin::Client, packet_r)},
            packet_r = serv.read_packet() => {(Origin::Server, packet_r)},
        };
        tracing::trace!("packet: {:?}", &packet_result);

        let mut packet = packet_result?;
        packet.resize();

        tracing::debug!("got packet: {}", packet.data.get_type_name());
        match &packet.data {
            PacketData::Tag { .. } => {
                if last_tag_packet.elapsed().as_millis() < 1000 {
                    use_udp = !use_udp;
                    tracing::info!("Using udp: {}", use_udp);
                }
                last_tag_packet = Instant::now();
            }
            PacketData::UdpInit { port } => {
                let addr = SocketAddr::new(serv_udp_addr.ip(), *port);
                tracing::debug!("New udp peer: {:?}", addr);

                udp.set_client_port(*port);

                serv.write_packet(&Packet::new(
                    Guid::default(),
                    PacketData::UdpInit { port: loc_udp_port },
                ))
                .await?;

                continue;
            }
            _ => {}
        }

        let (origin_conn, dest_conn) = match origin {
            Origin::Client => (&mut cli, &mut serv),
            Origin::Server => (&mut serv, &mut cli),
        };

        if use_udp && origin != plex {
            match packet.data {
                PacketData::Player { .. } => {
                    tracing::trace!("Sending over udp!");
                    udp.write_packet(&packet).await.unwrap();
                    continue;
                }
                _ => {}
            }
        }
        dest_conn.write_packet(&packet).await?
    }
}
