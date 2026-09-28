use core::hash::{BuildHasherDefault, Hasher};

/// A hash map using [`IdHasher`].
pub(crate) type HashMap<K, V> = hashbrown::HashMap<K, V, BuildHasherDefault<IdHasher>>;

/// A hash set using [`IdHasher`].
pub(crate) type HashSet<K> = hashbrown::HashSet<K, BuildHasherDefault<IdHasher>>;

/// A fast hasher for the solver's keys, which are all integers: the ids of symbols and variables,
/// and the addresses of constraints.
///
/// The solver doesn't depend on the iteration order of its maps, so unlike hashbrown's default
/// hasher, this one isn't randomly seeded. The keys aren't chosen by an attacker, so it doesn't
/// need to resist collisions either.
#[derive(Default, Clone, Copy)]
pub(crate) struct IdHasher(u64);

impl IdHasher {
    /// Mixes the bits of a value into the hash with a folded multiply, so that both the low bits
    /// (used by hashbrown to choose a bucket) and the high bits (used for its control bytes)
    /// depend on all the bits of the value. Addresses have their low bits set to zero.
    #[inline]
    fn add(&mut self, value: u64) {
        const MULTIPLIER: u64 = 0x9e37_79b9_7f4a_7c15;
        let product = u128::from(self.0 ^ value) * u128::from(MULTIPLIER);
        self.0 = (product as u64) ^ ((product >> 64) as u64);
    }
}

impl Hasher for IdHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.add(u64::from(byte));
        }
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use core::hash::{BuildHasher, Hash};

    use super::*;

    fn hash(value: impl Hash) -> u64 {
        BuildHasherDefault::<IdHasher>::default().hash_one(value)
    }

    #[test]
    fn spreads_aligned_values() {
        // addresses are aligned, so their low bits are zero, but the hash's low bits must differ
        let low_bits: hashbrown::HashSet<u64> = (0..64).map(|i| hash(i * 16usize) & 0x3f).collect();
        assert!(
            low_bits.len() > 32,
            "only {} different low bits",
            low_bits.len()
        );
    }
}
