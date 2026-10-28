use std::hash::{BuildHasherDefault, Hasher};

use indexmap::{IndexMap, IndexSet};

use crate::Id;

const K: u64 = 0xf135_7aea_2e62_a9c5;

#[derive(Clone, Copy, Debug, Default)]
pub struct IdHasher(u64);

impl IdHasher {
    #[inline]
    fn add(&mut self, v: u64) {
        self.0 = (self.0.rotate_left(26) ^ v).wrapping_mul(K);
    }
}

impl Hasher for IdHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            for (dst, src) in buf.iter_mut().zip(chunk) {
                *dst = *src;
            }
            self.add(u64::from_le_bytes(buf));
        }
    }

    #[inline]
    fn write_u8(&mut self, i: u8) {
        self.add(u64::from(i));
    }

    #[inline]
    fn write_u16(&mut self, i: u16) {
        self.add(u64::from(i));
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(u64::from(i));
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn write_isize(&mut self, i: isize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 32)
    }
}

pub type IdBuildHasher = BuildHasherDefault<IdHasher>;

pub type IdMap<V> = IndexMap<Id, V, IdBuildHasher>;

pub type IdSet = IndexSet<Id, IdBuildHasher>;

pub(crate) fn id_map<V>(n: usize) -> IdMap<V> {
    IdMap::with_capacity_and_hasher(n, IdBuildHasher::default())
}

pub(crate) fn id_set(n: usize) -> IdSet {
    IdSet::with_capacity_and_hasher(n, IdBuildHasher::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::BuildHasher;

    #[test]
    fn distinct_hashes_for_sequential_ids() {
        let b = IdBuildHasher::default();
        let mut seen = std::collections::HashSet::new();
        for i in 0u32..10_000 {
            assert!(seen.insert(b.hash_one(i)));
        }
        let mut m: IdMap<u32> = id_map(4);
        for i in 0..1000 {
            m.insert(i, i * 2);
        }
        assert_eq!(m.get(&500), Some(&1000));
        let mut s = id_set(0);
        s.insert(3);
        assert!(s.contains(&3));
        let mut h = IdHasher::default();
        h.write(b"hello world");
        assert_ne!(h.finish(), 0);
    }
}
