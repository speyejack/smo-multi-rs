use super::EncodingError;
use crate::net::encoding::{Decodable, Encodable};
use bytes::{Buf, BufMut};
use nalgebra::Quaternion as Q4;

pub type Quaternion = Q4<f32>;

impl<R> Decodable<R> for Quaternion
where
    R: Buf,
{
    fn decode(buf: &mut R) -> Result<Self, EncodingError> {
        Ok(Quaternion::new(
            buf.get_f32(),
            buf.get_f32(),
            buf.get_f32(),
            buf.get_f32(),
        ))
    }
}

impl<W> Encodable<W> for Quaternion
where
    W: BufMut,
{
    fn encode(&self, buf: &mut W) -> Result<(), EncodingError> {
        buf.put_f32(self.w);
        buf.put_f32(self.i);
        buf.put_f32(self.j);
        buf.put_f32(self.k);
        Ok(())
    }
}
