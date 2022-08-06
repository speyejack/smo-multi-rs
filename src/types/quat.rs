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
            buf.get_f32_le(),
            buf.get_f32_le(),
            buf.get_f32_le(),
            buf.get_f32_le(),
        ))
    }
}

impl<W> Encodable<W> for Quaternion
where
    W: BufMut,
{
    fn encode(&self, buf: &mut W) -> Result<(), EncodingError> {
        buf.put_f32_le(self.w);
        buf.put_f32_le(self.i);
        buf.put_f32_le(self.j);
        buf.put_f32_le(self.k);
        Ok(())
    }
}
