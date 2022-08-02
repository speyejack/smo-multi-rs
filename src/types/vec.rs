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
        vec.x = buf.get_f32();
        vec.y = buf.get_f32();
        vec.z = buf.get_f32();
        Ok(vec)
    }
}

impl<W> Encodable<W> for Vector3
where
    W: BufMut,
{
    fn encode(&self, buf: &mut W) -> Result<(), EncodingError> {
        buf.put_f32(self.x);
        buf.put_f32(self.y);
        buf.put_f32(self.z);
        Ok(())
    }
}
