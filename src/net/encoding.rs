use crate::types::EncodingError;
use bytes::{Buf, BufMut};

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
