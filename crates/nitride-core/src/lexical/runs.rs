//! Successful one-byte traces only; the generated loop still handles failure.

/// A generator-proven uniform ASCII trace, with no executed rule calls.
pub struct AsciiRun {
    mask: [u64; 2],
    #[cfg_attr(
        not(all(
            feature = "simd-ascii-runs",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )),
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
        #[cfg(all(feature = "simd-ascii-runs", target_arch = "aarch64"))]
        {
            count = if bytes.len() >= 16 && std::arch::is_aarch64_feature_detected!("neon") {
                // SAFETY: feature detection above and bounded slice loads inside.
                unsafe { self.neon(bytes) }
            } else {
                0
            };
        }
        #[cfg(not(all(
            feature = "simd-ascii-runs",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )))]
        {
            count = 0;
        }
        count
            + bytes[count..]
                .iter()
                .take_while(|b| self.contains(**b))
                .count()
    }
    #[cfg(all(feature = "simd-ascii-runs", target_arch = "aarch64"))]
    #[target_feature(enable = "neon")]
    unsafe fn neon(&self, bytes: &[u8]) -> usize {
        use std::arch::aarch64::*;
        // The low nibble selects a high-nibble membership mask. High nibbles
        // 8..15 select zero, so non-ASCII bytes stop the run before decoding.
        // SAFETY: both arrays have 16 initialized bytes; unaligned loads are valid.
        let low = unsafe { vld1q_u8(self.low.as_ptr()) };
        let high =
            unsafe { vld1q_u8([1, 2, 4, 8, 16, 32, 64, 128, 0, 0, 0, 0, 0, 0, 0, 0].as_ptr()) };
        // SAFETY: the lane index array has 16 initialized bytes.
        let lanes =
            unsafe { vld1q_u8([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15].as_ptr()) };
        let nibble = vdupq_n_u8(15);
        let zero = vdupq_n_u8(0);
        let mut at = 0;
        while bytes.len() - at >= 16 {
            // SAFETY: the loop ensures all 16 loaded bytes are within the slice.
            let input = unsafe { vld1q_u8(bytes.as_ptr().add(at)) };
            let lo = vqtbl1q_u8(low, vandq_u8(input, nibble));
            let hi = vqtbl1q_u8(high, vshrq_n_u8::<4>(input));
            let rejected = vceqq_u8(vandq_u8(lo, hi), zero);
            // Accepted lanes become 255; rejected lanes retain their index.
            // A minimum of 255 means the entire vector was accepted.
            let first = vminvq_u8(vorrq_u8(lanes, vmvnq_u8(rejected)));
            if first != 255 {
                return at + usize::from(first);
            }
            at += 16;
        }
        at
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
    #[cfg(all(feature = "simd-ascii-runs", target_arch = "aarch64"))]
    #[test]
    fn neon_vector_prefix_matches_membership_for_every_byte_and_mask() {
        if !std::arch::is_aarch64_feature_detected!("neon") {
            return;
        }
        let mut masks = vec![
            [0, 0],
            [u64::MAX; 2],
            [0x0123456789abcdef, 0xfedcba9876543210],
        ];
        for bit in 0..128 {
            let mut mask = [0; 2];
            mask[bit / 64] = 1 << (bit % 64);
            masks.push(mask);
        }
        for mask in masks {
            let plan = AsciiRun::new(mask, 1, 0);
            for byte in 0..=255u8 {
                let allowed = byte < 128 && mask[byte as usize / 64] & (1 << (byte % 64)) != 0;
                for length in [0, 1, 15, 16, 17, 31, 32, 33, 47, 48, 65] {
                    for offset in [0, 1, 15, 31] {
                        let bytes = vec![byte; offset + length];
                        let expected = if allowed { length / 16 * 16 } else { 0 };
                        // SAFETY: NEON availability was checked above.
                        let actual = unsafe { plan.neon(&bytes[offset..]) };
                        assert_eq!(
                            actual, expected,
                            "{mask:?} byte {byte} offset {offset} len {length}"
                        );
                        assert_eq!(
                            plan.scan(&bytes[offset..]),
                            if allowed { length } else { 0 }
                        );
                    }
                }
            }
        }
    }

    #[cfg(all(feature = "simd-ascii-runs", target_arch = "aarch64"))]
    #[test]
    fn neon_vector_prefix_stops_at_first_rejected_lane() {
        if !std::arch::is_aarch64_feature_detected!("neon") {
            return;
        }
        // Alternating accepted bytes use different low/high nibbles.
        let plan = AsciiRun::new([0, (1 << (b'a' - 64)) | (1 << (b'z' - 64))], 1, 0);
        for rejected in (0..=255u8).filter(|b| !matches!(b, b'a' | b'z')) {
            for length in [16, 17, 31, 32, 33, 47, 48, 65] {
                for offset in [0, 1, 7, 15] {
                    for at in 0..length {
                        let mut bytes: Vec<_> = (0..offset + length)
                            .map(|i| if i % 2 == 0 { b'a' } else { b'z' })
                            .collect();
                        bytes[offset + at] = rejected;
                        // SAFETY: NEON availability was checked above.
                        let actual = unsafe { plan.neon(&bytes[offset..]) };
                        assert_eq!(
                            actual,
                            at.min(length / 16 * 16),
                            "byte {rejected} at {at} offset {offset} len {length}"
                        );
                        assert_eq!(plan.scan(&bytes[offset..]), at);
                    }
                }
            }
        }
    }

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
