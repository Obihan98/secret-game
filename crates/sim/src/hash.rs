//! The daily state hash (TM9): our own 64-bit FNV-1a over an explicit little-endian encoding.
//!
//! Not `std::hash`: its algorithms and the bytes a type feeds them may change between Rust
//! versions, and the hash must stay comparable for a world's whole life. Each FNV-1a step is
//! a bijection on the state, so any single changed byte always changes the result.

use crate::codec::Sink;

const OFFSET_BASIS: u64 = 0xCBF2_9CE4_8422_2325;
const PRIME: u64 = 0x0000_0100_0000_01B3;

#[derive(Clone, Debug)]
pub struct StateHasher {
    state: u64,
}

impl Default for StateHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StateHasher {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: OFFSET_BASIS,
        }
    }

    /// Continues from a saved `finish()` value: FNV-1a's whole state is that one word, so a
    /// digest can be carried across a snapshot.
    #[must_use]
    pub const fn resume(state: u64) -> Self {
        Self { state }
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state = (self.state ^ u64::from(b)).wrapping_mul(PRIME);
        }
    }

    #[must_use]
    pub const fn finish(&self) -> u64 {
        self.state
    }
}

impl Sink for StateHasher {
    fn put_bytes(&mut self, bytes: &[u8]) {
        self.write_bytes(bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::StateHasher;
    use crate::codec::Sink;

    fn fnv(bytes: &[u8]) -> u64 {
        let mut h = StateHasher::new();
        h.write_bytes(bytes);
        h.finish()
    }

    #[test]
    fn matches_published_fnv1a_64_vectors() {
        assert_eq!(fnv(b""), 0xCBF2_9CE4_8422_2325);
        assert_eq!(fnv(b"a"), 0xAF63_DC4C_8601_EC8C);
        assert_eq!(fnv(b"foobar"), 0x8594_4171_F739_67E8);
    }

    #[test]
    fn integers_are_little_endian() {
        let mut h = StateHasher::new();
        h.put_u32(0x0403_0201);
        assert_eq!(h.finish(), fnv(&[1, 2, 3, 4]));
    }

    #[test]
    fn one_flipped_bit_changes_the_hash() {
        let base = [7u8; 64];
        for i in 0..base.len() * 8 {
            let mut flipped = base;
            flipped[i / 8] ^= 1 << (i % 8);
            assert_ne!(fnv(&flipped), fnv(&base), "bit {i}");
        }
    }
}
