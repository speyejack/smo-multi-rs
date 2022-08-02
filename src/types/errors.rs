use std::str::Utf8Error;

use crate::guid::Guid;
use thiserror::*;

#[derive(Error, Debug)]
pub enum SMOError {
    #[error("Invalid id")]
    InvalidID(Guid),

    #[error("Invalid encoding")]
    Encoding(#[from] EncodingError),
}

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("Not enough data")]
    NotEnoughData,
    #[error("Invalid string data")]
    StringError(#[from] Utf8Error),
    // Infallible(#[from] std::error::Infallible),
}
