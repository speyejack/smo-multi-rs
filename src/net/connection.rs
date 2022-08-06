use std::{io::Cursor, net::SocketAddr};

use bytes::{Buf, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::TcpStream,
};

use super::{encoding::Decodable, Packet};
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

            let read_amount = self.socket.read_buf(&mut self.buff).await?;
            if read_amount == 0 {
                if self.buff.is_empty() {
                    return Err(EncodingError::ConnectionClose.into());
                } else {
                    return Err(EncodingError::ConnectionReset.into());
                }
            }
        }
    }

    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        let mut buff = BytesMut::with_capacity(100);
        packet.encode(&mut buff)?;
        log::debug!("Writing packet: {:?}", packet);
        let mut amount = 0;
        while amount < buff.len() {
            let last_write = self.socket.write(&buff[..]).await?;
            amount += last_write;
        }
        self.socket.flush().await?;
        log::debug!("Packet written");
        Ok(())
    }
}
