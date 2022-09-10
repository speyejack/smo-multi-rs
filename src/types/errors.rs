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

    #[error("Invalid encoding: {0}")]
    Encoding(#[from] EncodingError),
    #[error("Bad IO")]
    Io(#[from] std::io::Error),
    #[error("Bad cli parsing")]
    Clap(#[from] clap::Error),
    #[error("Sending channel error")]
    SendChannel(#[from] Box<SendError<Command>>),
    #[error("Receiving channel error")]
    RecvChannel,
    #[error("Join error")]
    ThreadJoin(#[from] JoinError),
    #[error("Failed to initialize client: {0}")]
    ClientInit(#[from] ClientInitError),
    #[error("Invalid error")]
    JsonError(#[from] serde_json::Error),
    #[error("Udp not initialized")]
    UdpNotInit,
}

impl From<SendError<Command>> for SMOError {
    fn from(e: SendError<Command>) -> Self {
        Box::new(e).into()
    }
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

#[derive(Error, Debug)]
pub enum ClientInitError {
    #[error("Too many players already connected")]
    TooManyPlayers,
    #[error("Client IP address banned")]
    BannedIP,
    #[error("Client ID banned")]
    BannedID,
    #[error("Client handshake failed")]
    BadHandshake,
}

impl SMOError {
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            Self::Encoding(EncodingError::ConnectionClose)
            | Self::Encoding(EncodingError::ConnectionReset)
            | Self::RecvChannel => ErrorSeverity::ClientFatal,
            _ => ErrorSeverity::NonCritical,
        }
    }
}

pub enum ErrorSeverity {
    ServerFatal,
    ClientFatal,
    NonCritical,
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
