use std::{num::TryFromIntError, str::Utf8Error};

use crate::{
    cmds::{ClientCommand, Command, ServerWideCommand},
    guid::Guid,
};
use hex::FromHexError;
use serde::{de::Error as DeError, ser::Error as SerError};
use thiserror::*;
use tokio::{
    sync::{broadcast, mpsc::error::SendError, oneshot},
    task::JoinError,
};
pub type Result<T> = std::result::Result<T, SMOError>;

#[derive(Error, Debug)]
pub enum SMOError {
    #[error("Invalid id")]
    InvalidID(Guid),
    #[error("Invalid username")]
    InvalidName(String),
    #[error("Invalid console command argument: {0}")]
    InvalidConsoleArg(String),
    #[error("Invalid encoding: {0}")]
    Encoding(#[from] EncodingError),
    #[error("Bad IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Clap(#[from] clap::Error),
    #[error("Channel error")]
    Channel(Box<ChannelError>),
    #[error("Join error")]
    ThreadJoin(#[from] JoinError),
    #[error("Failed to initialize client: {0}")]
    ClientInit(#[from] ClientInitError),
    #[error("Invalid error")]
    JsonError(#[from] serde_json::Error),
    #[error("Udp not initialized")]
    UdpNotInit,
    #[error("Server being shutdown")]
    ServerShutdown,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl<T> From<T> for SMOError
where
    T: Into<ChannelError>,
{
    fn from(e: T) -> Self {
        SMOError::Channel(Box::new(e.into()))
    }
}

#[derive(Error, Debug)]
pub enum ChannelError {
    #[error("Sending channel error")]
    SendChannel(#[from] SendError<Command>),
    #[error("Sending client channel error")]
    SendClientChannel(#[from] SendError<ClientCommand>),

    #[error("Client broadcast channel sending error")]
    SendClientBroadcastChannel(#[from] broadcast::error::SendError<ClientCommand>),
    #[error("Server broadcast channel sending error")]
    SendServerBroadcastChannel(#[from] broadcast::error::SendError<ServerWideCommand>),
    #[error("Client broadcast channel receiving error")]
    RecvBroadcastChannel(#[from] broadcast::error::RecvError),

    #[error("Reply channel recv error")]
    ReplyChannel(#[from] oneshot::error::RecvError),
    #[error("Receiving error")]
    RecvChannel,
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
            | Self::Channel(_) => ErrorSeverity::ClientFatal,
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
