use std::{fmt::Display, str::FromStr};

use hex::FromHex;

use serde::{Deserialize, Serialize};

use crate::types::EncodingError;

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash, Copy, Default)]
pub struct Guid {
    pub id: [u8; 16],
}

impl TryFrom<&str> for Guid {
    type Error = EncodingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl FromStr for Guid {
    type Err = EncodingError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let str_rep = s.replace('-', "");
        let id = <[u8; 16]>::from_hex(str_rep)?;
        Ok(id.into())
    }
}

impl Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, digit) in self.id.iter().enumerate() {
            write!(f, "{:02x}", digit)?;
            match i {
                4 | 6 | 8 | 10 => write!(f, "-")?,
                _ => {}
            }
        }
        Ok(())
    }
}

impl From<[u8; 16]> for Guid {
    fn from(id: [u8; 16]) -> Self {
        Guid { id }
    }
}
