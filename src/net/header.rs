use serde::{Deserialize, Serialize};

use crate::guid::Guid;

#[derive(Serialize, Deserialize)]
pub struct PacketHeader {
    pub id: Guid,
    pub p_type: PacketType,
    pub data_size: u16,
}

#[derive(Serialize, Deserialize)]
pub enum PacketType {
    Unknown,
    Init,
    Player,
    Cap,
    Game,
    Tag,
    Connect,
    Disconnect,
    Costume,
    Shine,
    Capture,
    ChangeStage,
    Command,
}
