use std::{io::Cursor, net::SocketAddr};

use super::{encoding::Decodable, Packet, MAX_PACKET_SIZE};
use crate::{net::encoding::Encodable, types::Result};
use bytes::BytesMut;
use enet::{
    peer::{Packet as ENetPacket, Peer, PeerRecvEvent},
    protocol::PacketFlags,
};

#[derive(Debug)]
pub struct Connection {
    pub peer: Peer,
    pub addr: SocketAddr,
}

impl Connection {
    pub fn new(peer: Peer) -> Self {
        Connection {
            addr: peer.get_address(),
            peer,
        }
    }

    pub fn parse_packet(&mut self, buff: &[u8]) -> Result<Packet> {
        let mut buf = Cursor::new(buff);
        match Packet::check(&mut buf) {
            Ok(_) => {
                let len = buff.len();

                let packet = Packet::decode(&mut buf)?;

                Ok(packet)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub async fn read_packet(&mut self) -> Result<Packet> {
        loop {
            let event = self.peer.poll().await;
            match event {
                PeerRecvEvent::Recv(p) => return self.parse_packet(&p.data[..]),
                _ => {}
            }
        }
    }

    pub async fn write_packet(&mut self, packet: &Packet) -> Result<()> {
        let mut buff = BytesMut::with_capacity(MAX_PACKET_SIZE);
        packet.encode(&mut buff)?;
        let flags = match &packet.data {
            super::PacketData::Player { .. } | super::PacketData::Cap { .. } => {
                PacketFlags::reliable()
            }
            _ => PacketFlags::default(),
        };
        self.peer
            .send(ENetPacket {
                data: (&buff[..]).to_vec(),
                channel: 0,
                flags,
            })
            .await;
        Ok(())
    }
}
