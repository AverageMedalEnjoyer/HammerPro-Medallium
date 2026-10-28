use std::hash::Hasher;

const SEED: u64 = 0x5AFE_C0DE_1234_5678;

#[inline]
fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[derive(Clone, Debug)]
pub struct StableHasher {
    state: u64,
    tail: u64,
    tail_len: u32,
    len: u64,
}

impl Default for StableHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl StableHasher {
    pub fn new() -> Self {
        Self { state: SEED, tail: 0, tail_len: 0, len: 0 }
    }

    #[inline]
    fn word(&mut self, w: u64) {
        self.state = mix(self.state ^ w).wrapping_add(0x9E37_79B9_7F4A_7C15);
    }

    pub fn str(&mut self, s: &str) {
        self.write_u64(s.len() as u64);
        self.write(s.as_bytes());
    }

    pub fn str_ci(&mut self, s: &str) {
        self.write_u64(s.len() as u64);
        for chunk in s.as_bytes().chunks(8) {
            let mut buf = [0u8; 8];
            for (d, b) in buf.iter_mut().zip(chunk) {
                *d = b.to_ascii_lowercase();
            }
            self.write(buf.get(..chunk.len()).unwrap_or_default());
        }
    }

    pub fn f64(&mut self, v: f64) {
        let v = if v == 0.0 { 0.0 } else { v };
        self.write_u64(v.to_bits());
    }

    pub fn finish_u64(&self) -> u64 {
        let mut h = self.state;
        if self.tail_len > 0 {
            h = mix(h ^ self.tail ^ (u64::from(self.tail_len) << 56));
        }
        mix(h ^ self.len)
    }
}

impl Hasher for StableHasher {
    fn finish(&self) -> u64 {
        self.finish_u64()
    }

    fn write(&mut self, mut bytes: &[u8]) {
        self.len = self.len.wrapping_add(bytes.len() as u64);
        while self.tail_len > 0 && self.tail_len < 8 {
            let Some((&b, rest)) = bytes.split_first() else { return };
            self.tail |= u64::from(b) << (8 * self.tail_len);
            self.tail_len += 1;
            bytes = rest;
        }
        if self.tail_len == 8 {
            let w = self.tail;
            self.word(w);
            self.tail = 0;
            self.tail_len = 0;
        }
        let mut chunks = bytes.chunks_exact(8);
        for c in &mut chunks {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(c);
            self.word(u64::from_le_bytes(buf));
        }
        for &b in chunks.remainder() {
            self.tail |= u64::from(b) << (8 * self.tail_len);
            self.tail_len += 1;
        }
    }

    fn write_u64(&mut self, i: u64) {
        self.write(&i.to_le_bytes());
    }

    fn write_u32(&mut self, i: u32) {
        self.write(&i.to_le_bytes());
    }

    fn write_u8(&mut self, i: u8) {
        self.write(&[i]);
    }

    fn write_usize(&mut self, i: usize) {
        self.write_u64(i as u64);
    }

    fn write_i32(&mut self, i: i32) {
        self.write(&i.to_le_bytes());
    }
}

pub fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = StableHasher::new();
    h.write(bytes);
    h.finish_u64()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_values_and_chunking() {
        assert_eq!(hash_bytes(b""), hash_bytes(b""));
        let whole = hash_bytes(b"hello world, this is longer than a word");
        let mut h = StableHasher::new();
        for part in [&b"hel"[..], b"lo world, ", b"this is longer", b" than a word"] {
            h.write(part);
        }
        assert_eq!(h.finish_u64(), whole, "split writes give the same hash");
        assert_ne!(hash_bytes(b"a"), hash_bytes(b"b"));
        assert_ne!(hash_bytes(b"ab"), hash_bytes(b"ab\0"));
        let mut a = StableHasher::new();
        a.str("ab");
        a.str("c");
        let mut b = StableHasher::new();
        b.str("a");
        b.str("bc");
        assert_ne!(a.finish_u64(), b.finish_u64());
        let mut c = StableHasher::new();
        c.str_ci("Tools/ToolsNoDraw");
        let mut d = StableHasher::new();
        d.str_ci("TOOLS/toolsnodraw");
        assert_eq!(c.finish_u64(), d.finish_u64());
        let mut z = StableHasher::new();
        z.f64(-0.0);
        let mut p = StableHasher::new();
        p.f64(0.0);
        assert_eq!(z.finish_u64(), p.finish_u64());
        assert_eq!(hash_bytes(b"HammerPro"), 0x12d3_ffa0_030d_f61e);
        assert_eq!(hash_bytes(b""), 0xe8b2_9352_725c_6e45);
    }
}
