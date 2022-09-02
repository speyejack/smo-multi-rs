use std::{
    io::Cursor,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use bytes::{Buf, BufMut, BytesMut};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, BufWriter},
    net::{TcpStream, UdpSocket},
};

use crate::{
    net::{encoding::Decodable, encoding::Encodable, Packet, MAX_PACKET_SIZE},
    types::{EncodingError, Result, SMOError},
};

#[derive(Debug)]
pub enum UdpSenderStatus {
    Pending(IpAddr),
    Connected(SocketAddr),
}
#[derive(Debug)]
pub struct UdpConnection {
    pub socket: UdpSocket,
    pub buff: BytesMut,
    pub send_addr: UdpSenderStatus,
}

impl UdpConnection {
    pub fn new(stream: UdpSocket, addr: IpAddr) -> Self {
        UdpConnection {
            socket: stream,
            buff: BytesMut::with_capacity(1024),
            send_addr: UdpSenderStatus::Pending(addr),
        }
    }

    pub fn from_connection(stream: UdpSocket, addr: SocketAddr) -> Self {
        UdpConnection {
            socket: stream,
            buff: BytesMut::with_capacity(1024),
            send_addr: UdpSenderStatus::Connected(addr),
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

    pub fn is_client_udp(&self) -> bool {
        match self.send_addr {
            UdpSenderStatus::Connected(_) => true,
            _ => false,
        }
    }

    pub fn set_client_port(&mut self, port: u16) {
        let new_addr = match self.send_addr {
            UdpSenderStatus::Pending(ip) => SocketAddr::new(ip, port),
            UdpSenderStatus::Connected(addr) => {
                let ip = addr.ip();
                SocketAddr::new(ip, port)
            }
        };
        self.send_addr = UdpSenderStatus::Connected(new_addr)
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

        if let UdpSenderStatus::Connected(expected_addr) = self.send_addr {
            let (read_amount, addr) = self.socket.recv_from(&mut buff).await?;
            if addr == expected_addr {
                self.buff.put_slice(&buff[..read_amount]);
            }
        } else {
            // Never resolve as connection isnt ready
            futures::future::pending().await
        }

        Ok(())
    }

    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        tracing::trace!("Writing out udp packet: {:?}", packet);
        if let UdpSenderStatus::Connected(send_addr) = self.send_addr {
            let mut buff = BytesMut::with_capacity(MAX_PACKET_SIZE);
            packet.encode(&mut buff)?;

            let mut amount = 0;
            while amount < buff.len() {
                let last_write = self.socket.send_to(&buff[..], send_addr).await;
                amount += last_write.unwrap();
            }
            Ok(())
        } else {
            Err(SMOError::UdpNotInit)
        }
    }
}
