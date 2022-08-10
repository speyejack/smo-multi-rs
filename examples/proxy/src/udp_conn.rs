use std::{io::Cursor, net::SocketAddr};

use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::{TcpStream, UdpSocket},
};

use smoo::{
    net::{encoding::Decodable, encoding::Encodable, Packet, MAX_PACKET_SIZE},
    types::{EncodingError, Result},
};

#[derive(Debug)]
pub struct UdpConnection {
    pub socket: UdpSocket,
    pub addr: SocketAddr,
    pub buff: BytesMut,
}

impl UdpConnection {
    pub fn new(stream: UdpSocket) -> Self {
        UdpConnection {
            addr: stream.peer_addr().unwrap(),
            socket: stream,
            buff: BytesMut::with_capacity(1024),
        }
    }

    pub fn parse_packet(&mut self) -> Result<Option<Packet>> {
        let mut buf = Cursor::new(&self.buff[..]);
        match Packet::check(&mut buf) {
            Ok(_) => {
                let len = buf.position() as usize;

                buf.set_position(0);

                let packet = Packet::decode(&mut buf)?;
                self.buff.advance(len);

                Ok(Some(packet))
            }
            Err(EncodingError::NotEnoughData) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn read_packet(&mut self) -> Result<Packet> {
        loop {
            if let Some(packet) = self.parse_packet()? {
                return Ok(packet);
            }
            self.read_socket().await?
        }
    }

    pub async fn read_socket(&mut self) -> Result<()> {
        let _read_amount = self.socket.recv(&mut self.buff).await?;
        Ok(())
    }

    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        let mut buff = BytesMut::with_capacity(MAX_PACKET_SIZE);
        packet.encode(&mut buff)?;
        tracing::trace!("Writing packet: {:?}", packet);
        let mut amount = 0;
        while amount < buff.len() {
            let last_write = self.socket.send(&buff[..]).await?;
            amount += last_write;
        }
        tracing::trace!("Packet written");
        Ok(())
    }
}
