//! Successful one-byte traces only; the generated loop still handles failure.

/// A generator-proven uniform ASCII trace, with no executed rule calls.
pub struct AsciiRun {
    mask: [u64; 2],
    #[cfg_attr(
        not(all(feature = "simd-ascii-runs", target_arch = "x86_64")),
        allow(dead_code)
    )]
    low: [u8; 16],
    pub(super) work: u16,
    pub(super) depth: u8,
}
impl AsciiRun {
    pub const fn new(mask: [u64; 2], work: u16, depth: u8) -> Self {
        let mut low = [0; 16];
        let mut byte = 0;
        while byte < 128 {
            if mask[byte / 64] & (1u64 << (byte % 64)) != 0 {
                low[byte % 16] |= 1 << (byte / 16);
            }
            byte += 1;
        }
        Self {
            mask,
            low,
            work,
            depth,
        }
    }
    #[inline]
    fn contains(&self, byte: u8) -> bool {
        byte.is_ascii() && self.mask[byte as usize / 64] & (1u64 << (byte % 64)) != 0
    }
    /// Decline short runs before paying feature detection or replay overhead.
    #[inline]
    pub(super) fn prefix(&self, bytes: &[u8]) -> bool {
        bytes.len() >= 8 && bytes[..8].iter().all(|b| self.contains(*b))
    }
    pub(super) fn scan(&self, bytes: &[u8]) -> usize {
        let count;
        #[cfg(all(feature = "simd-ascii-runs", target_arch = "x86_64"))]
        {
            count = if bytes.len() >= 32 && std::is_x86_feature_detected!("avx2") {
                // SAFETY: feature detection above and bounded slice loads inside.
                unsafe { self.avx2(bytes) }
            } else {
                0
            };
        }
        #[cfg(not(all(feature = "simd-ascii-runs", target_arch = "x86_64")))]
        {
            count = 0;
        }
        count
            + bytes[count..]
                .iter()
                .take_while(|b| self.contains(**b))
                .count()
    }
    #[cfg(all(feature = "simd-ascii-runs", target_arch = "x86_64"))]
    #[target_feature(enable = "avx2")]
    unsafe fn avx2(&self, bytes: &[u8]) -> usize {
        use std::arch::x86_64::*;
        // Each low-nibble table entry is an eight-bit high-nibble membership
        // mask. A second shuffle selects the corresponding bit. Non-ASCII high
        // nibbles map to zero, so they stop before UTF-8 decoding is required.
        // SAFETY: low has 16 initialized bytes; unaligned loads are permitted.
        let low = _mm256_broadcastsi128_si256(unsafe { _mm_loadu_si128(self.low.as_ptr().cast()) });
        let high = _mm256_setr_epi8(
            1, 2, 4, 8, 16, 32, 64, -128, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 4, 8, 16, 32, 64, -128, 0,
            0, 0, 0, 0, 0, 0, 0,
        );
        let nibble = _mm256_set1_epi8(15);
        let zero = _mm256_setzero_si256();
        let mut at = 0;
        while bytes.len() - at >= 32 {
            // SAFETY: the loop ensures all 32 loaded bytes are in the slice.
            let input = unsafe { _mm256_loadu_si256(bytes.as_ptr().add(at).cast()) };
            let lo = _mm256_shuffle_epi8(low, input);
            let hi =
                _mm256_shuffle_epi8(high, _mm256_and_si256(_mm256_srli_epi16(input, 4), nibble));
            let rejected =
                _mm256_movemask_epi8(_mm256_cmpeq_epi8(_mm256_and_si256(lo, hi), zero)) as u32;
            if rejected != 0 {
                return at + rejected.trailing_zeros() as usize;
            }
            at += 32;
        }
        at
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classifier_matches_all_bytes_masks_boundaries_and_alignments() {
        let masks = [
            [0, 0],
            [u64::MAX, u64::MAX],
            [0x0123456789abcdef, 0xfedcba9876543210],
            [0x03ff000000000000, 0],
        ];
        for mask in masks {
            let plan = AsciiRun::new(mask, 1, 0);
            for byte in 0..=255u8 {
                for offset in [0, 1, 15, 31] {
                    for length in [0, 1, 7, 8, 15, 31, 32, 33, 63, 64, 65, 97] {
                        let bytes = vec![byte; offset + length];
                        let slice = &bytes[offset..];
                        let expected = slice.iter().take_while(|b| plan.contains(**b)).count();
                        assert_eq!(
                            plan.scan(slice),
                            expected,
                            "{mask:?} byte {byte} offset {offset} len {length}"
                        );
                    }
                }
            }
        }
        for allowed in 0..128u8 {
            let mut mask = [u64::MAX; 2];
            mask[allowed as usize / 64] &= !(1u64 << (allowed % 64));
            let plan = AsciiRun::new(mask, 1, 0);
            let fill = if allowed == b'a' { b'b' } else { b'a' };
            for at in 0..97 {
                let mut bytes = vec![fill; 97];
                bytes[at] = allowed;
                assert_eq!(plan.scan(&bytes), at);
            }
        }
    }
}
