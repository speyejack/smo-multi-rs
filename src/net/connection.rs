use std::{io::Cursor, net::SocketAddr};

use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
};

use super::{encoding::Decodable, AnyPacket, MAX_PACKET_SIZE};
use crate::{
    net::encoding::Encodable,
    types::{EncodingError, Result},
};

#[derive(Debug)]
pub struct Connection {
    pub addr: SocketAddr,
    pub socket: BufWriter<TcpStream>,
    pub buff: BytesMut,
}

impl Connection {
    pub fn new(stream: TcpStream) -> Self {
        Connection {
            addr: stream.peer_addr().unwrap(),
            socket: BufWriter::new(stream),
            buff: BytesMut::with_capacity(1024),
        }
    }

    pub fn parse_packet(&mut self) -> Result<Option<AnyPacket>> {
        let mut buf = Cursor::new(&self.buff[..]);
        match AnyPacket::check(&mut buf) {
            Ok(_) => {
                let len = buf.position() as usize;

                buf.set_position(0);

                let packet = AnyPacket::decode(&mut buf)?;
                self.buff.advance(len);

                Ok(Some(packet))
            }
            Err(EncodingError::NotEnoughData) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub async fn read_packet(&mut self) -> Result<AnyPacket> {
        loop {
            if let Some(packet) = self.parse_packet()? {
                return Ok(packet);
            }

            self.read_socket().await?
        }
    }

    pub async fn read_socket(&mut self) -> Result<()> {
        let read_amount = self.socket.read_buf(&mut self.buff).await?;
        if read_amount == 0 {
            if self.buff.is_empty() {
                Err(EncodingError::ConnectionClose.into())
            } else {
                Err(EncodingError::ConnectionReset.into())
            }
        } else {
            Ok(())
        }
    }

    pub async fn write_packet(&mut self, packet: &AnyPacket) -> Result<()> {
        let mut buff = BytesMut::with_capacity(MAX_PACKET_SIZE);
        packet.encode(&mut buff)?;
        tracing::trace!("Writing packet: {:?}", packet);
        let mut amount = 0;
        while amount < buff.len() {
            let last_write = self.socket.write(&buff[..]).await?;
            amount += last_write;
        }
        self.socket.flush().await?;
        tracing::trace!("Packet written");
        Ok(())
    }
}
