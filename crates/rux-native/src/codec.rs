//! The binary wire format shared with the generated Kotlin / Swift code
//! (`RuxWriter` / `RuxReader` in the native runtimes). Little-endian:
//!
//! | Type | Encoding |
//! |---|---|
//! | `bool` | 1 byte, 0 / 1 |
//! | `i32` / `i64` / `f64` | 4 / 8 / 8 bytes |
//! | `String` | `u32` byte length + UTF-8 |
//! | `Vec<T>` | `u32` count + items |
//! | `Option<T>` | 1 byte tag (0 = none, 1 = some) + value |
//! | struct (`#[derive(NativeType)]`) | fields in declaration order |
//! | enum (`#[derive(NativeType)]`, unit variants) | `i32` variant index |
//! | `()` | nothing |

use std::fmt;

/// A value that can cross to native code.
pub trait Codec: Sized {
    fn encode(&self, w: &mut Writer);
    fn decode(r: &mut Reader) -> Result<Self, DecodeError>;
}

#[derive(Default)]
pub struct Writer(Vec<u8>);

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn finish(self) -> Vec<u8> {
        self.0
    }

    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }

    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }

    pub fn bytes(&mut self, v: &[u8]) {
        self.0.extend_from_slice(v);
    }
}

pub struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], DecodeError> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.bytes.len());
        let end = end.ok_or_else(|| DecodeError::new("unexpected end of data"))?;
        let out = &self.bytes[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(self.take(1)?[0])
    }

    pub fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }

    /// Fails unless every byte was consumed.
    pub fn finish(self) -> Result<(), DecodeError> {
        if self.pos == self.bytes.len() {
            Ok(())
        } else {
            Err(DecodeError::new("trailing data"))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError(String);

impl DecodeError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self(msg.into())
    }
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Codec for () {
    fn encode(&self, _: &mut Writer) {}
    fn decode(_: &mut Reader) -> Result<Self, DecodeError> {
        Ok(())
    }
}

impl Codec for bool {
    fn encode(&self, w: &mut Writer) {
        w.u8(*self as u8);
    }
    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        match r.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(DecodeError::new(format!("invalid bool {b}"))),
        }
    }
}

macro_rules! number {
    ($($t:ty),*) => {$(
        impl Codec for $t {
            fn encode(&self, w: &mut Writer) {
                w.bytes(&self.to_le_bytes());
            }
            fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
                Ok(<$t>::from_le_bytes(r.take(size_of::<$t>())?.try_into().unwrap()))
            }
        }
    )*};
}
number!(i32, i64, f64);

impl Codec for String {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.len() as u32);
        w.bytes(self.as_bytes());
    }
    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        let len = r.u32()? as usize;
        String::from_utf8(r.take(len)?.to_vec()).map_err(|_| DecodeError::new("invalid UTF-8"))
    }
}

impl<T: Codec> Codec for Option<T> {
    fn encode(&self, w: &mut Writer) {
        match self {
            None => w.u8(0),
            Some(v) => {
                w.u8(1);
                v.encode(w);
            }
        }
    }
    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        match r.u8()? {
            0 => Ok(None),
            1 => Ok(Some(T::decode(r)?)),
            b => Err(DecodeError::new(format!("invalid option tag {b}"))),
        }
    }
}

impl<T: Codec> Codec for Vec<T> {
    fn encode(&self, w: &mut Writer) {
        w.u32(self.len() as u32);
        for v in self {
            v.encode(w);
        }
    }
    fn decode(r: &mut Reader) -> Result<Self, DecodeError> {
        let len = r.u32()? as usize;
        (0..len).map(|_| T::decode(r)).collect()
    }
}

/// Encodes one value.
pub fn encode<T: Codec>(value: &T) -> Vec<u8> {
    let mut w = Writer::new();
    value.encode(&mut w);
    w.finish()
}

/// Decodes one value, requiring all bytes to be consumed.
pub fn decode<T: Codec>(bytes: &[u8]) -> Result<T, DecodeError> {
    let mut r = Reader::new(bytes);
    let v = T::decode(&mut r)?;
    r.finish()?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};

    #[test]
    fn round_trips() {
        let v: (Vec<Option<String>>, i64) = (vec![Some("hé".into()), None], -7);
        let bytes = encode(&v.0);
        assert_eq!(decode::<Vec<Option<String>>>(&bytes).unwrap(), v.0);
        assert_eq!(decode::<i64>(&encode(&v.1)).unwrap(), -7);
        assert_eq!(decode::<f64>(&encode(&1.5f64)).unwrap(), 1.5);
        assert!(decode::<bool>(&encode(&true)).unwrap());
        assert_eq!(encode(&3i32), [3, 0, 0, 0]);
        assert_eq!(encode(&"ab".to_string()), [2, 0, 0, 0, b'a', b'b']);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(decode::<i32>(&[1, 2]).is_err());
        assert!(decode::<bool>(&[2]).is_err());
        assert!(decode::<i32>(&[1, 0, 0, 0, 9]).is_err()); // trailing
        assert!(decode::<String>(&[5, 0, 0, 0, b'a']).is_err());
    }
}
