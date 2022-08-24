use std::{io::Cursor, net::SocketAddr};

use bytes::{Buf, BufMut, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::{TcpStream, UdpSocket},
};

use crate::{
    net::{encoding::Decodable, encoding::Encodable, Packet, MAX_PACKET_SIZE},
    types::{EncodingError, Result},
};

#[derive(Debug)]
pub struct UdpConnection {
    pub socket: UdpSocket,
    pub buff: BytesMut,
    pub send_addr: SocketAddr,
}

impl UdpConnection {
    pub fn new(stream: UdpSocket, addr: SocketAddr) -> Self {
        UdpConnection {
            socket: stream,
            buff: BytesMut::with_capacity(1024),
            send_addr: addr,
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
        let mut buff = vec![0u8; 100];

        let read_amount = self.socket.recv_from(&mut buff).await?;
        self.buff.put_slice(&buff[..read_amount.0]);
        Ok(())
    }

    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        let mut buff = BytesMut::with_capacity(MAX_PACKET_SIZE);
        packet.encode(&mut buff)?;

        let mut amount = 0;
        while amount < buff.len() {
            let last_write = self.socket.send_to(&buff[..], self.send_addr).await;
            amount += last_write.unwrap();
        }
        Ok(())
    }
}
