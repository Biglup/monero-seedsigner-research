//! Minimal reader/writer for monero's `serialization/binary_archive.h` wire
//! format: raw fixed-size blobs, little-endian fixed integers, CryptoNote
//! varints, one-byte bools, and containers as varint count + elements.

use crate::SignerError;

pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], SignerError> {
        if self.remaining() < n {
            return Err(SignerError::Malformed("unexpected end of data"));
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    pub fn array32(&mut self) -> Result<[u8; 32], SignerError> {
        let mut out = [0u8; 32];
        out.copy_from_slice(self.bytes(32)?);
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8, SignerError> {
        Ok(self.bytes(1)?[0])
    }

    pub fn bool(&mut self) -> Result<bool, SignerError> {
        Ok(self.u8()? != 0)
    }

    pub fn u32_le(&mut self) -> Result<u32, SignerError> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn u64_le(&mut self) -> Result<u64, SignerError> {
        let b = self.bytes(8)?;
        let mut a = [0u8; 8];
        a.copy_from_slice(b);
        Ok(u64::from_le_bytes(a))
    }

    pub fn varint(&mut self) -> Result<u64, SignerError> {
        let mut v: u64 = 0;
        let mut shift = 0;
        loop {
            let b = self.u8()?;
            if shift >= 64 {
                return Err(SignerError::Malformed("varint too long"));
            }
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
        }
    }

    pub fn count(&mut self) -> Result<usize, SignerError> {
        let n = self.varint()?;
        if n > self.remaining() as u64 {
            return Err(SignerError::Malformed("container count exceeds data"));
        }
        Ok(n as usize)
    }

    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }
}

#[derive(Default)]
pub struct Writer {
    pub out: Vec<u8>,
}

impl Writer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn bytes(&mut self, b: &[u8]) {
        self.out.extend_from_slice(b);
    }

    pub fn u8(&mut self, v: u8) {
        self.out.push(v);
    }

    pub fn bool(&mut self, v: bool) {
        self.out.push(v as u8);
    }

    pub fn u32_le(&mut self, v: u32) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64_le(&mut self, v: u64) {
        self.out.extend_from_slice(&v.to_le_bytes());
    }

    pub fn varint(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.out.push((v as u8 & 0x7f) | 0x80);
            v >>= 7;
        }
        self.out.push(v as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        for v in [0u64, 1, 127, 128, 300, 16383, 16384, u32::MAX as u64, u64::MAX] {
            let mut w = Writer::new();
            w.varint(v);
            let mut r = Reader::new(&w.out);
            assert_eq!(r.varint().unwrap(), v);
            assert!(r.is_empty());
        }
    }
}
