use std::str::Utf8Error;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use thiserror::Error;

pub trait Encodable<W>
where
    Self: Sized,
    W: BufMut,
{
    fn encode(&self, buf: &mut W) -> Result<(), EncodingError>;
}

pub trait Decodable<R>
where
    Self: Sized,
    R: Buf,
{
    fn decode(buf: &mut R) -> Result<Self, EncodingError>;
}

#[derive(Error, Debug)]
pub enum EncodingError {
    #[error("Not enough data")]
    NotEnoughData,
    #[error("Invalid string data")]
    StringError(#[from] Utf8Error),
    // Infallible(#[from] std::error::Infallible),
}
