use bytes::BytesMut;
use smoo::net::connection::Connection;
use smoo::net::encoding::Encodable;
use smoo::types::Result;
use std::{io::Cursor, net::SocketAddr};
use tokio;
use tokio::net::{TcpListener, TcpSocket, TcpStream};
use tracing::Instrument;
use tracing_subscriber::EnvFilter;

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
        tracing::info!("new client connection: {}", addr);
        let span = tracing::info_span!("cli", addr = addr.ip().to_string());
        tokio::spawn(
            async move {
                let result = proxy_client(from_socket).await;
                if let Err(e) = result {
                    tracing::warn!("Client error: {e}");
                }
            }
            .instrument(span),
        );
    }

    Ok(())
}

enum Origin {
    Client,
    Server,
}

async fn proxy_client(cli_sock: TcpStream) -> Result<()> {
    let server_conn = tokio::net::TcpSocket::new_v4()?;
    let addr = "127.0.0.1:61888".parse().unwrap();
    let serv_sock = server_conn.connect(addr).await?;

    let mut cli = Connection::new(cli_sock);
    let mut serv = Connection::new(serv_sock);

    tracing::info!("Client setup and ready");
    loop {
        let (origin, packet_result) = tokio::select! {
            packet_r = cli.read_packet() => {(Origin::Client, packet_r)},
            packet_r = serv.read_packet() => {(Origin::Server, packet_r)},
        };
        tracing::debug!("packet: {:?}", &packet_result);

        let packet = packet_result?;

        let current_conn = match origin {
            Origin::Client => &mut cli,
            Origin::Server => &mut serv,
        };

        // let mut buff = BytesMut::with_capacity(300);
        // packet.encode(&mut buff);
        // assert_eq!(&current_conn.last_data[..].len(), &buff[..].len());

        tracing::info!("got packet: {}", packet.data.get_type_name());

        match origin {
            Origin::Client => serv.write_packet(&packet).await?,
            Origin::Server => cli.write_packet(&packet).await?,
        }
    }
}
