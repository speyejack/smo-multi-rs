use std::{num::TryFromIntError, str::Utf8Error};

use crate::{cmds::Command, guid::Guid};
use hex::FromHexError;
use serde::{de::Error as DeError, ser::Error as SerError};
use thiserror::*;
use tokio::{sync::mpsc::error::SendError, task::JoinError};
pub type Result<T> = std::result::Result<T, SMOError>;

#[derive(Error, Debug)]
pub enum SMOError {
    #[error("Invalid id")]
    InvalidID(Guid),

    #[error("Invalid encoding")]
    Encoding(#[from] EncodingError),
    #[error("Bad IO")]
    Io(#[from] std::io::Error),
    #[error("Bad cli parsing")]
    Clap(#[from] clap::Error),
    #[error("Communication error")]
    Channel(#[from] SendError<Command>),
    #[error("Join error")]
    Join(#[from] JoinError),
    #[error("Failed to initialize client")]
    ClientInit,
}

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("Not enough data")]
    NotEnoughData,
    #[error("Invalid string data")]
    BadUtf8(#[from] Utf8Error),
    #[error("Invalid integer conversion")]
    IntConversion(#[from] TryFromIntError),
    #[error("Invalid hex conversion")]
    HexConversion(#[from] FromHexError),
    #[error("Connection reset by peer")]
    ConnectionReset,
    #[error("Connection closed by peer")]
    ConnectionClose,
    #[error("Serde error")]
    CustomError,
}

impl SerError for EncodingError {
    fn custom<T>(_msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        EncodingError::CustomError
    }
}

impl DeError for EncodingError {
    fn custom<T>(_msg: T) -> Self
    where
        T: std::fmt::Display,
    {
        EncodingError::CustomError
    }
}
