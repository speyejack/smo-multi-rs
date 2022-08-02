use std::{convert::Infallible, fmt::Display, str::FromStr};

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, PartialEq, Clone, Eq, Hash, Copy)]
pub struct Guid {
    pub id: [u8; 16],
}

impl Default for Guid {
    fn default() -> Self {
        Self { id: [0; 16] }
    }
}

impl FromStr for Guid {
    type Err = Infallible;

    fn from_str(_: &str) -> Result<Self, Self::Err> {
        unimplemented!()
    }
}

impl Display for Guid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        todo!()
    }
}
