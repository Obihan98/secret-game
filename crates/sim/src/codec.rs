//! Byte encoding for snapshots and log records (TS6): fixed-width little-endian, written by
//! hand like the state hash, so no dependency update can ever change what an old file means.
//!
//! Decoding is strict: every tag and length is checked and trailing bytes are an error, so two
//! different byte strings never decode to the same value. The daily log digest relies on that.

use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    UnexpectedEnd,
    TrailingBytes,
    /// Written by a format version this build cannot read.
    UnsupportedVersion(u16),
    BadTag(u8),
    /// Well-formed bytes that describe an impossible state (e.g. an ID out of range).
    Invalid(&'static str),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnexpectedEnd => f.write_str("unexpected end of data"),
            Self::TrailingBytes => f.write_str("trailing bytes after the end"),
            Self::UnsupportedVersion(v) => write!(f, "unsupported format version {v}"),
            Self::BadTag(t) => write!(f, "unknown tag {t}"),
            Self::Invalid(what) => write!(f, "invalid data: {what}"),
        }
    }
}

impl std::error::Error for DecodeError {}

#[derive(Debug, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    #[must_use]
    pub const fn new() -> Self {
        Self { buf: Vec::new() }
    }

    pub fn put_u8(&mut self, n: u8) {
        self.buf.push(n);
    }

    pub fn put_u16(&mut self, n: u16) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    pub fn put_u32(&mut self, n: u32) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    pub fn put_u64(&mut self, n: u64) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    pub fn put_i64(&mut self, n: i64) {
        self.buf.extend_from_slice(&n.to_le_bytes());
    }

    /// Lengths go in as `u64` so the bytes are the same on 32- and 64-bit hosts.
    pub fn put_len(&mut self, n: usize) {
        self.put_u64(n as u64);
    }

    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.buf
    }
}

#[derive(Debug)]
pub struct Reader<'a> {
    bytes: &'a [u8],
}

impl<'a> Reader<'a> {
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], DecodeError> {
        let (head, rest) = self
            .bytes
            .split_first_chunk::<N>()
            .ok_or(DecodeError::UnexpectedEnd)?;
        self.bytes = rest;
        Ok(*head)
    }

    /// # Errors
    /// `UnexpectedEnd` if the data runs out.
    pub fn u8(&mut self) -> Result<u8, DecodeError> {
        Ok(u8::from_le_bytes(self.take()?))
    }

    /// # Errors
    /// `UnexpectedEnd` if the data runs out.
    pub fn u16(&mut self) -> Result<u16, DecodeError> {
        Ok(u16::from_le_bytes(self.take()?))
    }

    /// # Errors
    /// `UnexpectedEnd` if the data runs out.
    pub fn u32(&mut self) -> Result<u32, DecodeError> {
        Ok(u32::from_le_bytes(self.take()?))
    }

    /// # Errors
    /// `UnexpectedEnd` if the data runs out.
    pub fn u64(&mut self) -> Result<u64, DecodeError> {
        Ok(u64::from_le_bytes(self.take()?))
    }

    /// # Errors
    /// `UnexpectedEnd` if the data runs out.
    pub fn i64(&mut self) -> Result<i64, DecodeError> {
        Ok(i64::from_le_bytes(self.take()?))
    }

    /// A table length. Callers must not preallocate from it: corrupt data can claim any size.
    ///
    /// # Errors
    /// `UnexpectedEnd` if the data runs out, `Invalid` if it does not fit in `usize`.
    pub fn read_len(&mut self) -> Result<usize, DecodeError> {
        usize::try_from(self.u64()?).map_err(|_| DecodeError::Invalid("length"))
    }

    /// # Errors
    /// `TrailingBytes` if anything is left unread.
    pub fn finish(self) -> Result<(), DecodeError> {
        if self.bytes.is_empty() {
            Ok(())
        } else {
            Err(DecodeError::TrailingBytes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_little_endian() {
        let mut w = Writer::new();
        w.put_u8(1);
        w.put_u16(0x0302);
        w.put_u32(0x0706_0504);
        w.put_i64(-1);
        let bytes = w.into_bytes();
        assert_eq!(bytes[..7], [1, 2, 3, 4, 5, 6, 7]);

        let mut r = Reader::new(&bytes);
        assert_eq!(r.u8(), Ok(1));
        assert_eq!(r.u16(), Ok(0x0302));
        assert_eq!(r.u32(), Ok(0x0706_0504));
        assert_eq!(r.i64(), Ok(-1));
        assert_eq!(r.finish(), Ok(()));
    }

    #[test]
    fn short_and_long_input_are_errors() {
        assert_eq!(
            Reader::new(&[1, 2, 3]).u32(),
            Err(DecodeError::UnexpectedEnd)
        );
        let mut r = Reader::new(&[1, 2]);
        assert_eq!(r.u8(), Ok(1));
        assert_eq!(r.finish(), Err(DecodeError::TrailingBytes));
    }
}
