use serde::Serialize;

use crate::net::{fixedStr::FixedString, COSTUME_NAME_SIZE};

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Costume {
    pub body_name: FixedString<COSTUME_NAME_SIZE>,
    pub cap_name: FixedString<COSTUME_NAME_SIZE>,
}

impl Default for Costume {
    fn default() -> Self {
        Self {
            body_name: "Mario".to_string().into(),
            cap_name: "Mario".to_string().into(),
        }
    }
}
