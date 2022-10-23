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
        let i = buf.get_f32_le();
        let j = buf.get_f32_le();
        let k = buf.get_f32_le();
        let w = buf.get_f32_le();
        Ok(Quaternion::new(w, i, j, k))
    }
}

impl<W> Encodable<W> for Quaternion
where
    W: BufMut,
{
    fn encode(&self, buf: &mut W) -> Result<(), EncodingError> {
        buf.put_f32_le(self.i);
        buf.put_f32_le(self.j);
        buf.put_f32_le(self.k);
        buf.put_f32_le(self.w);
        Ok(())
    }
}
