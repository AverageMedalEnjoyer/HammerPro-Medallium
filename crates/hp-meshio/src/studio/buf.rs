use glam::DVec3;

#[derive(Default)]
pub(crate) struct Buf {
    pub b: Vec<u8>,
    strings: Vec<(usize, usize, String)>,
}

impl Buf {
    pub fn pos(&self) -> usize {
        self.b.len()
    }
    pub fn i32(&mut self, v: i32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u16(&mut self, v: u16) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u8(&mut self, v: u8) {
        self.b.push(v);
    }
    pub fn f32(&mut self, v: f32) {
        self.b.extend_from_slice(&v.to_le_bytes());
    }
    pub fn vec3(&mut self, v: DVec3) {
        self.f32(v.x as f32);
        self.f32(v.y as f32);
        self.f32(v.z as f32);
    }
    pub fn zeros(&mut self, n: usize) {
        self.b.resize(self.b.len() + n, 0);
    }
    pub fn align(&mut self, n: usize) {
        while !self.b.len().is_multiple_of(n) {
            self.b.push(0);
        }
    }
    pub fn set_i32(&mut self, at: usize, v: i32) {
        self.b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    pub fn str_ref(&mut self, base: usize, text: &str) {
        self.strings.push((self.pos(), base, text.to_owned()));
        self.i32(0);
    }
    pub fn str_at(&mut self, at: usize, base: usize, text: &str) {
        self.strings.push((at, base, text.to_owned()));
    }
    pub fn fixed_str(&mut self, text: &str, size: usize) {
        let bytes = text.as_bytes();
        let n = bytes.len().min(size.saturating_sub(1));
        self.b.extend_from_slice(&bytes[..n]);
        self.zeros(size - n);
    }
    pub fn write_strings(&mut self) {
        let mut placed: Vec<(String, usize)> = Vec::new();
        for (field, base, text) in std::mem::take(&mut self.strings) {
            let at = match placed.iter().find(|(t, _)| *t == text) {
                Some((_, at)) => *at,
                None => {
                    let at = self.pos();
                    self.b.extend_from_slice(text.as_bytes());
                    self.b.push(0);
                    placed.push((text, at));
                    at
                }
            };
            self.set_i32(field, at as i32 - base as i32);
        }
    }
}

pub(crate) fn checksum(parts: &[&[u8]]) -> i32 {
    let mut h: u32 = 0x811c_9dc5;
    for p in parts {
        for &b in *p {
            h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
        }
    }
    (h | 1) as i32
}
