use std::{fmt::Debug, io::Cursor};

use super::encoding::{Decodable, Encodable};
use crate::{
    guid::Guid,
    types::{Costume, EncodingError, Quaternion, Vector3},
};
use bytes::{Buf, BufMut};

type Result<T> = std::result::Result<T, EncodingError>;

pub const MAX_PACKET_SIZE: usize = 300;

const COSTUME_NAME_SIZE: usize = 0x20;
const CAP_ANIM_SIZE: usize = 0x30;
const STAGE_GAME_NAME_SIZE: usize = 0x40;
const STAGE_CHANGE_NAME_SIZE: usize = 0x30;
const STAGE_ID_SIZE: usize = 0x10;
const CLIENT_NAME_SIZE: usize = COSTUME_NAME_SIZE;

#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub id: Guid,
    pub data_size: u16,
    pub data: PacketData,
}

impl Packet {
    pub fn new(id: Guid, data: PacketData) -> Packet {
        Packet {
            id,
            data_size: data
                .get_size()
                .try_into()
                .expect("Extremely large data size"),
            data,
        }
    }

    pub fn resize(&mut self) {
        self.data_size = self.data.get_size() as u16;
    }

    pub fn check(buf: &mut Cursor<&[u8]>) -> Result<u64> {
        let header_size = 16 + 2;
        let start_pos = buf.position();
        if buf.remaining() < header_size + 2 {
            return Err(EncodingError::NotEnoughData);
        }

        buf.advance(header_size);
        let size = buf.get_u16_le().into();
        if buf.remaining() < size {
            return Err(EncodingError::NotEnoughData);
        }
        buf.advance(size);
        Ok(buf.position() - start_pos)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PacketData {
    Unhandled {
        tag: u16,
        data: Vec<u8>,
    },
    Init {
        max_players: u16,
    },
    Player {
        pos: Vector3,
        rot: Quaternion,
        animation_blend_weights: [f32; 6],
        act: u16,
        sub_act: u16,
    },
    Cap {
        pos: Vector3,
        rot: Quaternion,
        cap_out: bool,
        cap_anim: String,
    },
    Game {
        is_2d: bool,
        scenario_num: i8,
        stage: String,
    },
    Tag {
        update_type: TagUpdate,
        is_it: bool,
        seconds: u8,
        minutes: u16,
    },
    Connect {
        c_type: ConnectionType,
        max_player: u16,
        client_name: String,
    },
    Disconnect,
    Costume(Costume),
    Shine {
        shine_id: i32,
        is_grand: bool,
    },
    Capture {
        model: String,
    },
    ChangeStage {
        stage: String,
        id: String,
        scenario: i8,
        sub_scenario: u8,
    },
    Command,
    UdpInit {
        port: u16,
    },
    HolePunch,
}

impl PacketData {
    fn get_size(&self) -> usize {
        match self {
            Self::Unhandled { data, .. } => data.len(),
            Self::Init { .. } => 2,
            Self::Player { .. } => 0x38,
            Self::Cap { .. } => 29 + CAP_ANIM_SIZE,
            Self::Game { .. } => 2 + STAGE_GAME_NAME_SIZE,
            Self::Tag { .. } => 5,
            Self::Connect { .. } => 6 + CLIENT_NAME_SIZE,
            Self::Disconnect { .. } => 0,
            Self::Costume { .. } => COSTUME_NAME_SIZE * 2,
            Self::Shine { .. } => 5,
            Self::Capture { .. } => COSTUME_NAME_SIZE,
            Self::ChangeStage { .. } => STAGE_ID_SIZE + STAGE_CHANGE_NAME_SIZE + 2,
            Self::Command { .. } => 0,
            Self::UdpInit { .. } => 2,
            Self::HolePunch { .. } => 0,
        }
    }

    fn get_type_id(&self) -> u16 {
        match self {
            Self::Unhandled { tag, .. } => *tag,
            Self::Init { .. } => 1,
            Self::Player { .. } => 2,
            Self::Cap { .. } => 3,
            Self::Game { .. } => 4,
            Self::Tag { .. } => 5,
            Self::Connect { .. } => 6,
            Self::Disconnect { .. } => 7,
            Self::Costume { .. } => 8,
            Self::Shine { .. } => 9,
            Self::Capture { .. } => 10,
            Self::ChangeStage { .. } => 11,
            Self::Command { .. } => 12,
            Self::UdpInit { .. } => 13,
            Self::HolePunch { .. } => 14,
        }
    }

    pub fn get_type_name(&self) -> String {
        match self {
            Self::Unhandled { .. } => "unhandled",
            Self::Init { .. } => "init",
            Self::Player { .. } => "player",
            Self::Cap { .. } => "cap",
            Self::Game { .. } => "game",
            Self::Tag { .. } => "tag",
            Self::Connect { .. } => "connect",
            Self::Disconnect { .. } => "disconnect",
            Self::Costume { .. } => "costume",
            Self::Shine { .. } => "shine",
            Self::Capture { .. } => "capture",
            Self::ChangeStage { .. } => "changeStage",
            Self::Command { .. } => "command",
            Self::UdpInit { .. } => "udpInit",
            Self::HolePunch { .. } => "holePunch",
        }
        .to_string()
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionType {
    FirstConnection,
    Reconnecting,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagUpdate {
    Time = 1,
    State = 2,
}

impl TagUpdate {}

impl<R> Decodable<R> for Packet
where
    R: Buf,
{
    fn decode(buf: &mut R) -> std::result::Result<Self, EncodingError> {
        if buf.remaining() < (16 + 2 + 2) {
            return Err(EncodingError::NotEnoughData);
        }

        let mut id = [0; 16];
        buf.copy_to_slice(&mut id);
        let p_type = buf.get_u16_le();
        let p_size = buf.get_u16_le();

        if buf.remaining() < p_size.into() {
            return Err(EncodingError::NotEnoughData);
        }

        let data = match p_type {
            1 => PacketData::Init {
                max_players: buf.get_u16_le(),
            },
            2 => PacketData::Player {
                // pos: Vector3::new(buf.get_f32_le(), buf.get_f32_le(), buf.get_f32_le()),
                pos: Vector3::decode(buf)?,
                rot: Quaternion::decode(buf)?,
                animation_blend_weights: {
                    let mut weights = [0.0; 6];
                    for weight in &mut weights {
                        *weight = buf.get_f32_le();
                    }
                    weights
                },
                act: buf.get_u16_le(),
                sub_act: buf.get_u16_le(),
            },
            3 => PacketData::Cap {
                pos: Vector3::decode(buf)?,
                rot: Quaternion::decode(buf)?,
                cap_out: buf.get_u8() != 0,
                cap_anim: std::str::from_utf8(&buf.copy_to_bytes(COSTUME_NAME_SIZE)[..])?
                    .to_string(),
            },
            4 => PacketData::Game {
                is_2d: buf.get_u8() != 0,
                scenario_num: buf.get_i8(),
                stage: buf_size_to_string(buf, STAGE_GAME_NAME_SIZE)?,
            },
            5 => PacketData::Tag {
                update_type: if buf.get_u8() == 1 {
                    TagUpdate::Time
                } else {
                    TagUpdate::State
                },
                is_it: buf.get_u8() != 0,
                seconds: buf.get_u8(),
                minutes: buf.get_u16_le(),
            },
            6 => {
                let c_type = if buf.get_u32_le() == 0 {
                    ConnectionType::FirstConnection
                } else {
                    ConnectionType::Reconnecting
                };
                let max_player = buf.get_u16_le();
                let client_name = buf_size_to_string(buf, CLIENT_NAME_SIZE)?;
                PacketData::Connect {
                    c_type,
                    max_player,
                    client_name,
                }
            }
            7 => PacketData::Disconnect,
            8 => PacketData::Costume(Costume {
                body_name: buf_size_to_string(buf, COSTUME_NAME_SIZE)?,
                cap_name: buf_size_to_string(buf, COSTUME_NAME_SIZE)?,
            }),
            9 => PacketData::Shine {
                shine_id: buf.get_i32_le(),
                is_grand: buf.get_u8() != 0,
            },
            10 => PacketData::Capture {
                model: buf_size_to_string(buf, COSTUME_NAME_SIZE)?,
            },
            11 => PacketData::ChangeStage {
                stage: buf_size_to_string(buf, STAGE_CHANGE_NAME_SIZE)?,
                id: buf_size_to_string(buf, STAGE_ID_SIZE)?,
                scenario: buf.get_i8(),
                sub_scenario: buf.get_u8(),
            },
            12 => PacketData::Command {},
            13 => PacketData::UdpInit {
                port: buf.get_u16_le(),
            },
            14 => PacketData::HolePunch {},
            _ => PacketData::Unhandled {
                tag: p_type,
                data: buf.copy_to_bytes(p_size.into())[..].to_vec(),
            },
        };

        let excess_padding = p_size as usize - data.get_size();
        if excess_padding > 0 {
            buf.advance(excess_padding);
        }

        Ok(Packet {
            id: id.into(),
            data_size: p_size,
            data,
        })
    }
}

impl<W> Encodable<W> for Packet
where
    W: BufMut,
{
    fn encode(&self, buf: &mut W) -> Result<()> {
        buf.put_slice(&self.id.id[..]);
        buf.put_u16_le(self.data.get_type_id());
        buf.put_u16_le(self.data_size);
        match &self.data {
            PacketData::Unhandled { data, .. } => buf.put_slice(&data[..]),
            PacketData::Init { max_players } => {
                buf.put_u16_le(*max_players);
            }
            PacketData::Player {
                pos,
                rot,
                animation_blend_weights,
                act,
                sub_act,
            } => {
                pos.encode(buf)?;
                rot.encode(buf)?;
                for weight in animation_blend_weights {
                    buf.put_f32_le(*weight);
                }
                buf.put_u16_le(*act);
                buf.put_u16_le(*sub_act);
            }
            PacketData::Cap {
                pos,
                rot,
                cap_out,
                cap_anim,
            } => {
                pos.encode(buf)?;
                rot.encode(buf)?;
                buf.put_u8((*cap_out).into());
                buf.put_slice(&str_to_sized_array::<CAP_ANIM_SIZE>(cap_anim));
            }
            PacketData::Game {
                is_2d,
                scenario_num,
                stage,
            } => {
                buf.put_u8((*is_2d).into());
                buf.put_i8(*scenario_num);
                buf.put_slice(&str_to_sized_array::<STAGE_GAME_NAME_SIZE>(stage));
            }
            PacketData::Tag {
                update_type,
                is_it,
                seconds,
                minutes,
            } => {
                let tag = match update_type {
                    TagUpdate::Time => 1,
                    TagUpdate::State => 2,
                };
                buf.put_u8(tag);
                buf.put_u8((*is_it).into());
                buf.put_u8(*seconds);
                buf.put_u16_le(*minutes);
            }
            PacketData::Connect {
                c_type,
                max_player,
                client_name,
            } => {
                let tag = match c_type {
                    ConnectionType::FirstConnection => 0,
                    ConnectionType::Reconnecting => 1,
                };
                buf.put_u32_le(tag);
                buf.put_u16_le(*max_player);
                buf.put_slice(&str_to_sized_array::<CLIENT_NAME_SIZE>(client_name));
            }
            PacketData::Disconnect => {}
            PacketData::Costume(Costume {
                body_name,
                cap_name,
            }) => {
                buf.put_slice(&str_to_sized_array::<COSTUME_NAME_SIZE>(body_name));
                buf.put_slice(&str_to_sized_array::<COSTUME_NAME_SIZE>(cap_name));
            }
            PacketData::Shine { shine_id, is_grand } => {
                buf.put_i32_le(*shine_id);
                buf.put_u8(if *is_grand { 1 } else { 0 });
            }
            PacketData::Capture { model } => {
                buf.put_slice(&str_to_sized_array::<COSTUME_NAME_SIZE>(model))
            }
            PacketData::ChangeStage {
                stage,
                id,
                scenario,
                sub_scenario,
            } => {
                buf.put_slice(&str_to_sized_array::<STAGE_CHANGE_NAME_SIZE>(stage));
                buf.put_slice(&str_to_sized_array::<STAGE_ID_SIZE>(id));
                buf.put_i8(*scenario);
                buf.put_u8(*sub_scenario);
            }
            PacketData::Command => {}
            PacketData::UdpInit { port } => {
                buf.put_u16_le(*port);
            }
            PacketData::HolePunch => {}
        }

        Ok(())
    }
}

fn str_to_sized_array<const N: usize>(s: &str) -> [u8; N] {
    let mut bytes = [0; N];
    for (b, c) in bytes.iter_mut().zip(s.as_bytes()) {
        *b = *c;
    }
    bytes
}

fn buf_size_to_string(buf: &mut impl Buf, size: usize) -> Result<String> {
    Ok(std::str::from_utf8(&buf.copy_to_bytes(size)[..])?
        .trim_matches(char::from(0))
        .to_string())
}

#[cfg(test)]
mod test {

    use super::*;
    use bytes::BytesMut;
    use quickcheck::{quickcheck, Arbitrary};

    impl Arbitrary for Packet {
        fn arbitrary(g: &mut quickcheck::Gen) -> Self {
            let options = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13];
            let mut buff = BytesMut::with_capacity(MAX_PACKET_SIZE);

            let enum_num = g.choose(&options).unwrap();
            let size = match enum_num {
                1 => 2,
                2 => 0x38,
                3 => 29 + CAP_ANIM_SIZE,
                4 => 2 + STAGE_GAME_NAME_SIZE,
                5 => 5,
                6 => 6 + CLIENT_NAME_SIZE,
                7 => 0,
                8 => COSTUME_NAME_SIZE * 2,
                9 => 5,
                10 => COSTUME_NAME_SIZE,
                11 => STAGE_ID_SIZE + STAGE_CHANGE_NAME_SIZE + 2,
                12 => 0,
                13 => 2,
                14 => 0,
                _ => 0,
            };

            for _ in 0..16 {
                buff.put_u8(u8::arbitrary(g));
            }
            buff.put_u16_le(*enum_num);
            buff.put_u16_le(size as u16);
            for _ in 0..size {
                buff.put_u8(u8::arbitrary(g) % 128);
            }

            let mut packet = Packet::decode(&mut Cursor::new(buff)).unwrap();
            packet.resize();
            packet
        }
    }

    quickcheck! {
        fn round_trip(p: Packet) -> bool {
            let mut buff = BytesMut::with_capacity(1000);

            p.encode(&mut buff).map(|_| Packet::decode(&mut buff).map(|de_p| de_p == p).unwrap_or(false)).unwrap_or(false)
        }
    }
}
