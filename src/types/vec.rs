use super::EncodingError;
use crate::net::encoding::{Decodable, Encodable};
use bytes::{Buf, BufMut};
use nalgebra::Vector3 as V3;

pub type Vector3 = V3<f32>;

impl<R> Decodable<R> for Vector3
where
    R: Buf,
{
    fn decode(buf: &mut R) -> Result<Self, EncodingError> {
        let mut vec = Self::default();
        vec.x = buf.get_f32_le();
        vec.y = buf.get_f32_le();
        vec.z = buf.get_f32_le();
        Ok(vec)
    }
}

impl<W> Encodable<W> for Vector3
where
    W: BufMut,
{
    fn encode(&self, buf: &mut W) -> Result<(), EncodingError> {
        buf.put_f32_le(self.x);
        buf.put_f32_le(self.y);
        buf.put_f32_le(self.z);
        Ok(())
    }
}
