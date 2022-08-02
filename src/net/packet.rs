use std::mem::Discriminant;

use super::encoding::{Decodable, Encodable};
use crate::{
    guid::Guid,
    types::{Costume, EncodingError, Quaternion, Vector3},
};
use bytes::{Buf, BufMut};
const COSTUME_NAME_SIZE: usize = 0x20;
const STAGE_NAME_SIZE: usize = 0x30;
const STAGE_ID_SIZE: usize = 0x10;
const CLIENT_NAME_SIZE: usize = COSTUME_NAME_SIZE;

#[derive(Debug, Clone)]
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
}

#[derive(Debug, Clone)]
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
        scenario_num: u8,
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
    },
    Capture {
        model: String,
    },
    ChangeStage {
        stage: String,
        id: String,
        scenerio: i8,
        sub_scenario: u8,
    },
    Command,
}

impl PacketData {
    fn get_size(&self) -> usize {
        match self {
            Self::Unhandled { data, .. } => data.len(),
            Self::Init { .. } => 2,
            Self::Player { .. } => 0x38,
            Self::Cap { .. } => 0x50,
            Self::Game { .. } => 0x42,
            Self::Tag { .. } => 6,
            Self::Connect { .. } => 6 + CLIENT_NAME_SIZE,
            Self::Disconnect { .. } => 0,
            Self::Costume { .. } => COSTUME_NAME_SIZE * 2,
            Self::Shine { .. } => 4,
            Self::Capture { .. } => COSTUME_NAME_SIZE,
            Self::ChangeStage { .. } => STAGE_ID_SIZE + STAGE_NAME_SIZE + 2,
            Self::Command { .. } => 0,
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
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum ConnectionType {
    FirstConnection,
    Reconnecting,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum TagUpdate {
    Time = 1,
    State = 2,
}

impl TagUpdate {}

impl<R> Decodable<R> for Packet
where
    R: Buf,
{
    fn decode(mut buf: &mut R) -> std::result::Result<Self, EncodingError> {
        if buf.remaining() < (16 + 2 + 2) {
            return Err(EncodingError::NotEnoughData);
        }

        let mut id = [0; 16];
        buf.copy_to_slice(&mut id);
        let p_type = buf.get_u16();
        let p_size = buf.get_u16();

        if buf.remaining() < p_size.into() {
            return Err(EncodingError::NotEnoughData);
        }

        let data = match p_type {
            1 => PacketData::Init {
                max_players: buf.get_u16(),
            },
            2 => PacketData::Player {
                // pos: Vector3::new(buf.get_f32(), buf.get_f32(), buf.get_f32()),
                pos: Vector3::decode(buf)?,
                rot: Quaternion::decode(buf)?,
                animation_blend_weights: {
                    let mut weights = [0.0; 6];
                    for weight in &mut weights {
                        *weight = buf.get_f32();
                    }
                    weights
                },
                act: buf.get_u16(),
                sub_act: buf.get_u16(),
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
                scenario_num: buf.get_u8(),
                stage: std::str::from_utf8(&buf.copy_to_bytes(COSTUME_NAME_SIZE)[..])?.to_string(),
            },
            5 => PacketData::Tag {
                update_type: if buf.get_u8() == 1 {
                    TagUpdate::Time
                } else {
                    TagUpdate::State
                },
                is_it: buf.get_u8() != 0,
                seconds: buf.get_u8(),
                minutes: buf.get_u16(),
            },
            6 => PacketData::Connect {
                c_type: if buf.get_u8() == 0 {
                    ConnectionType::FirstConnection
                } else {
                    ConnectionType::Reconnecting
                },
                max_player: buf.get_u16(),
                client_name: std::str::from_utf8(&buf.copy_to_bytes(CLIENT_NAME_SIZE)[..])?
                    .to_string(),
            },
            7 => PacketData::Disconnect,
            8 => PacketData::Costume(Costume {
                body_name: std::str::from_utf8(&buf.copy_to_bytes(COSTUME_NAME_SIZE)[..])?
                    .to_string(),
                cap_name: std::str::from_utf8(&buf.copy_to_bytes(COSTUME_NAME_SIZE)[..])?
                    .to_string(),
            }),
            9 => PacketData::Shine {
                shine_id: buf.get_i32(),
            },
            10 => PacketData::Capture {
                model: std::str::from_utf8(&buf.copy_to_bytes(COSTUME_NAME_SIZE)[..])?.to_string(),
            },
            11 => PacketData::ChangeStage {
                stage: std::str::from_utf8(&buf.copy_to_bytes(STAGE_NAME_SIZE)[..])?.to_string(),
                id: std::str::from_utf8(&buf.copy_to_bytes(STAGE_ID_SIZE)[..])?.to_string(),
                scenerio: buf.get_i8(),
                sub_scenario: buf.get_u8(),
            },
            12 => PacketData::Command {},
            _ => PacketData::Unhandled {
                tag: p_type,
                data: buf.copy_to_bytes(p_size.into())[..].to_vec(),
            },
        };

        todo!()
    }
}

impl<W> Encodable<W> for Packet
where
    W: BufMut,
{
    fn encode(&self, buf: &mut W) -> Result<(), EncodingError> {
        buf.put_slice(&self.id.id[..]);
        buf.put_u16(self.data.get_type_id());
        buf.put_u16(self.data_size);
        match &self.data {
            PacketData::Unhandled { data, .. } => buf.put_slice(&data[..]),
            PacketData::Init { max_players } => {
                buf.put_u16(*max_players);
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
                    buf.put_f32(*weight);
                }
                buf.put_u16(*act);
                buf.put_u16(*sub_act);
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
                buf.put_slice(&cap_anim.as_bytes()[..COSTUME_NAME_SIZE])
            }
            PacketData::Game {
                is_2d,
                scenario_num,
                stage,
            } => {
                buf.put_u8((*is_2d).into());
                buf.put_u8(*scenario_num);
                buf.put_slice(&stage.as_bytes()[..COSTUME_NAME_SIZE])
            }
            PacketData::Tag {
                update_type,
                is_it,
                seconds,
                minutes,
            } => {
                let tag = match update_type {
                    TagUpdate::Time => 0,
                    TagUpdate::State => 1,
                };
                buf.put_u8(tag);
                buf.put_u8((*is_it).into());
                buf.put_u8(*seconds);
                buf.put_u16(*minutes);
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
                buf.put_u8(tag);
                buf.put_u16(*max_player);
                buf.put_slice(&client_name.as_bytes()[..COSTUME_NAME_SIZE])
            }
            PacketData::Disconnect => {}
            PacketData::Costume(Costume {
                body_name,
                cap_name,
            }) => {
                buf.put_slice(&body_name.as_bytes()[..COSTUME_NAME_SIZE]);
                buf.put_slice(&cap_name.as_bytes()[..COSTUME_NAME_SIZE]);
            }
            PacketData::Shine { shine_id } => buf.put_i32(*shine_id),
            PacketData::Capture { model } => buf.put_slice(&model.as_bytes()[..COSTUME_NAME_SIZE]),
            PacketData::ChangeStage {
                stage,
                id,
                scenerio,
                sub_scenario,
            } => {
                buf.put_slice(&stage.as_bytes()[..COSTUME_NAME_SIZE]);
                buf.put_slice(&id.as_bytes()[..COSTUME_NAME_SIZE]);
                buf.put_i8(*scenerio);
                buf.put_u8(*sub_scenario);
            }
            PacketData::Command => {}
        }
        Ok(())
    }
}
