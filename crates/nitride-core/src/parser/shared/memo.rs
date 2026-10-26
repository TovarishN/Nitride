//! Full memo identity with one contiguous randomized hash input.
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct MemoKey {
    position: u32,
    minimum: u32,
    rule: u16,
    context: u8,
}
impl MemoKey {
    #[inline]
    pub(super) fn new(rule: u16, position: u32, minimum: u32, context: u8) -> Self {
        Self {
            position,
            minimum,
            rule,
            context,
        }
    }
}
impl Hash for MemoKey {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Fixed-width encoding includes every semantic bit, without padding.
        let position = self.position.to_le_bytes();
        let minimum = self.minimum.to_le_bytes();
        let rule = self.rule.to_le_bytes();
        state.write(&[
            position[0],
            position[1],
            position[2],
            position[3],
            minimum[0],
            minimum[1],
            minimum[2],
            minimum[3],
            rule[0],
            rule[1],
            self.context,
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::MemoKey;
    use std::collections::HashMap;
    use std::hash::{BuildHasherDefault, Hasher};

    #[derive(Default)]
    struct CollisionHasher;
    impl Hasher for CollisionHasher {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, _: &[u8]) {}
    }

    #[test]
    fn memo_keys_keep_every_field_distinct_under_hash_collisions() {
        let values = [
            (0, 0, 0, 0),
            (u16::MAX, 0, 0, 0),
            (0, u32::MAX, 0, 0),
            (0, 0, u32::MAX, 0),
            (0, 0, 0, 1),
            (0, 0, 0, 2),
            (0, 0, 0, 0x80),
            (u16::MAX, u32::MAX, u32::MAX, u8::MAX),
            (1, 0x0001_0000, 0x0100_0000, 2),
            (1, 0x0100_0000, 0x0001_0000, 2),
        ];
        let mut map = HashMap::<_, _, BuildHasherDefault<CollisionHasher>>::default();
        for (index, &(rule, position, minimum, context)) in values.iter().enumerate() {
            assert!(
                map.insert(MemoKey::new(rule, position, minimum, context), index)
                    .is_none()
            );
        }
        for (index, &(rule, position, minimum, context)) in values.iter().enumerate() {
            assert_eq!(
                map.get(&MemoKey::new(rule, position, minimum, context)),
                Some(&index)
            );
        }
        assert_eq!(map.len(), values.len());
    }

    #[test]
    fn memo_key_keeps_existing_cache_entry_size() {
        assert_eq!(
            std::mem::size_of::<MemoKey>(),
            std::mem::size_of::<(u16, u32, u32, u8)>()
        );
        assert_eq!(
            std::mem::size_of::<(MemoKey, super::super::Memo)>(),
            std::mem::size_of::<((u16, u32, u32, u8), super::super::Memo)>()
        );
    }
}
