use crate::net::encoding::{Decodable, Encodable, EncodingError};
use bytes::{Buf, BufMut};
use nalgebra::{Quaternion as Q4, Vector3 as V3};

pub type Vector3 = V3<f32>;
pub type Quaternion = Q4<f32>;

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
