use std::{fmt::{self, Display, Debug}, str::FromStr};


use crate::types::EncodingError;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameMode {
    Legacy      =  0,
    HideAndSeek =  1,
    Sardines    =  2,
    FreezeTag   =  3,
    Unknown04   =  4,
    Unknown05   =  5,
    Unknown06   =  6,
    Unknown07   =  7,
    Unknown08   =  8,
    Unknown09   =  9,
    Unknown10   = 10,
    Unknown11   = 11,
    Unknown12   = 12,
    Unknown13   = 13,
    Reserved    = 14, // reserved for possible extensions (indicating an extra byte for future gamemodes)
    None        = 15, // == -1
}

impl GameMode {
    pub fn from_u8(x: u8) -> Self {
        match x {
             0 => GameMode::Legacy,
             1 => GameMode::HideAndSeek,
             2 => GameMode::Sardines,
             3 => GameMode::FreezeTag,
             4 => GameMode::Unknown04,
             5 => GameMode::Unknown05,
             6 => GameMode::Unknown06,
             7 => GameMode::Unknown07,
             8 => GameMode::Unknown08,
             9 => GameMode::Unknown09,
            10 => GameMode::Unknown10,
            11 => GameMode::Unknown11,
            12 => GameMode::Unknown12,
            13 => GameMode::Unknown13,
            14 => GameMode::Reserved,
             _ => GameMode::None,
        }
    }
    pub fn to_u8(x: Self) -> u8 {
        match x {
            GameMode::Legacy      =>  0,
            GameMode::HideAndSeek =>  1,
            GameMode::Sardines    =>  2,
            GameMode::FreezeTag   =>  3,
            GameMode::Unknown04   =>  4,
            GameMode::Unknown05   =>  5,
            GameMode::Unknown06   =>  6,
            GameMode::Unknown07   =>  7,
            GameMode::Unknown08   =>  8,
            GameMode::Unknown09   =>  9,
            GameMode::Unknown10   => 10,
            GameMode::Unknown11   => 11,
            GameMode::Unknown12   => 12,
            GameMode::Unknown13   => 13,
            GameMode::Reserved    => 14,
            GameMode::None        => 15,
        }
    }
    pub fn from_i8(x: i8) -> Self {
        GameMode::from_u8((x as u8) & 0x0f)
    }
    pub fn to_i8(x: Self) -> i8 {
        (((GameMode::to_u8(x) + 1) as i8) % 16) - 1
    }
}

impl TryFrom<&str> for GameMode {
    type Error = EncodingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl FromStr for GameMode {
    type Err = EncodingError;
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input {
          "-1" | "None"        => Ok(GameMode::None),
          "0"  | "Legacy"      => Ok(GameMode::Legacy),
          "1"  | "HideAndSeek" => Ok(GameMode::HideAndSeek),
          "2"  | "Sardines"    => Ok(GameMode::Sardines),
          "3"  | "FreezeTag"   => Ok(GameMode::FreezeTag),
          "4"|"5"|"6"|"7"|"8"|"9"|"10"|"11"|"12"|"13"|"14" => Ok(GameMode::from_u8(input.parse().unwrap())),
          _ => Err(EncodingError::CustomError),
        }
    }
}

impl Display for GameMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self, f)
    }
}

impl From<GameMode> for String {
    fn from(game_mode: GameMode) -> Self {
        game_mode.to_string()
    }
}

impl TryFrom<String> for GameMode {
    type Error = EncodingError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}
