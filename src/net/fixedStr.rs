use std::ops::Deref;

use bytes::{Buf, BufMut, BytesMut};
use serde::{de::Visitor, Deserialize, Serialize};

use crate::types::EncodingError;

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FixedString<const N: usize>(String);

impl<const N: usize> FixedString<N> {
    pub fn new(s: String) -> Self {
        Self(s)
    }
}

impl<const N: usize> Deref for FixedString<N> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const N: usize> AsRef<String> for FixedString<N> {
    fn as_ref(&self) -> &String {
        &self.0
    }
}

impl<'de, const N: usize> Deserialize<'de> for FixedString<N> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_tuple(N, FixedStringVisitor::new())
    }
}

impl<const N: usize> Serialize for FixedString<N> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let array = str_to_sized_array::<N>(&self.0);
        array.serialize(serializer)
    }
}

impl<const N: usize> Into<FixedString<N>> for String {
    fn into(self) -> FixedString<N> {
        FixedString(self)
    }
}

impl<const N: usize> Into<String> for FixedString<N> {
    fn into(self) -> String {
        return self.0;
    }
}

pub fn str_to_sized_array<const N: usize>(s: &str) -> [u8; N] {
    let mut bytes = [0; N];
    for (b, c) in bytes.iter_mut().zip(s.as_bytes()) {
        *b = *c;
    }
    bytes
}

pub fn buf_size_to_string(buf: &mut impl Buf, size: usize) -> Result<String, EncodingError> {
    Ok(std::str::from_utf8(&buf.copy_to_bytes(size)[..])?
        .trim_matches(char::from(0))
        .to_string())
}

struct FixedStringVisitor<const N: usize>;

impl<const N: usize> FixedStringVisitor<N> {
    fn new() -> Self {
        Self
    }
}

impl<const N: usize> Visitor<'_> for FixedStringVisitor<N> {
    type Value = FixedString<N>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("A sequence of bytes {self.N} long")
    }
}
