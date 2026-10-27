//! Fast hashing for parser-private maps. Their keys are fixed-width positions,
//! rule ids, pointers and grammar label names, never source text. A random seed per map keeps
//! bucket placement unpredictable, as the standard randomized hasher does.
use std::hash::{BuildHasher, Hasher};

const MULTIPLIER: u64 = 0x9e37_79b9_7f4a_7c15;
const FINISH: u64 = 0xa076_1d64_78bd_642f;

#[inline]
fn fold(a: u64, b: u64) -> u64 {
    let product = u128::from(a) * u128::from(b);
    (product as u64) ^ ((product >> 64) as u64)
}

#[derive(Clone, Copy)]
pub(crate) struct Seeded {
    seed: u64,
}
impl Default for Seeded {
    fn default() -> Self {
        Self {
            seed: std::collections::hash_map::RandomState::new().hash_one(0_u64) | 1,
        }
    }
}
impl BuildHasher for Seeded {
    type Hasher = Fold;
    #[inline]
    fn build_hasher(&self) -> Fold {
        Fold { state: self.seed }
    }
}

pub(crate) struct Fold {
    state: u64,
}
impl Hasher for Fold {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // Callers hash fixed-width keys, so zero padding cannot make two
        // distinct keys of one type equal.
        for chunk in bytes.chunks(8) {
            let mut word = [0; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }
    #[inline]
    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }
    #[inline]
    fn write_u16(&mut self, value: u16) {
        self.write_u64(u64::from(value));
    }
    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }
    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.state = fold(self.state ^ value, MULTIPLIER);
    }
    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
    #[inline]
    fn finish(&self) -> u64 {
        fold(self.state, FINISH)
    }
}

pub(crate) type FastMap<K, V> = std::collections::HashMap<K, V, Seeded>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::Hash;

    fn hash<T: Hash>(builder: &Seeded, value: T) -> u64 {
        builder.hash_one(value)
    }

    #[test]
    fn sequential_keys_spread_over_high_bits() {
        // The standard map selects buckets by low bits and control bytes by
        // the top seven bits; sequential positions must vary in both.
        let builder = Seeded::default();
        let mut top = std::collections::HashSet::new();
        let mut low = std::collections::HashSet::new();
        for position in 0..4096_u32 {
            let value = hash(&builder, (position, 7_u16));
            top.insert(value >> 57);
            low.insert(value & 0xfff);
        }
        assert_eq!(top.len(), 128);
        assert!(low.len() > 2400, "{}", low.len());
    }

    #[test]
    fn seeds_differ_between_maps() {
        assert_ne!(
            hash(&Seeded::default(), 1_u64),
            hash(&Seeded::default(), 1_u64)
        );
    }

    #[test]
    fn every_key_field_changes_the_hash() {
        let builder = Seeded::default();
        let base = hash(&builder, (1_u32, 2_u16, 3_u8));
        assert_ne!(base, hash(&builder, (0_u32, 2_u16, 3_u8)));
        assert_ne!(base, hash(&builder, (1_u32, 0_u16, 3_u8)));
        assert_ne!(base, hash(&builder, (1_u32, 2_u16, 0_u8)));
    }
}
