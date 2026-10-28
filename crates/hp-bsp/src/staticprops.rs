use crate::error::{BspError, Result};

pub const NAME_LEN: usize = 128;
pub const TARGETNAME_LEN: usize = 128;

pub const FLAG_NO_PER_TEXEL_LIGHTING: u32 = 0x100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StaticPropFormat {
    V4,
    V5,
    V5Ship,
    V6,
    V6Bgt,
    V7L4d,
    V8,
    V9,
    V10,
    V10Csgo,
    V11Lite,
    V11BlackMesa,
    V11Csgo,
}

impl StaticPropFormat {
    pub const ALL: [StaticPropFormat; 13] = [
        StaticPropFormat::V4,
        StaticPropFormat::V5,
        StaticPropFormat::V5Ship,
        StaticPropFormat::V6,
        StaticPropFormat::V6Bgt,
        StaticPropFormat::V7L4d,
        StaticPropFormat::V8,
        StaticPropFormat::V9,
        StaticPropFormat::V10,
        StaticPropFormat::V10Csgo,
        StaticPropFormat::V11Lite,
        StaticPropFormat::V11BlackMesa,
        StaticPropFormat::V11Csgo,
    ];

    pub const fn version(self) -> u16 {
        match self {
            StaticPropFormat::V4 => 4,
            StaticPropFormat::V5 | StaticPropFormat::V5Ship => 5,
            StaticPropFormat::V6 | StaticPropFormat::V6Bgt => 6,
            StaticPropFormat::V7L4d => 7,
            StaticPropFormat::V8 => 8,
            StaticPropFormat::V9 => 9,
            StaticPropFormat::V10 | StaticPropFormat::V10Csgo => 10,
            StaticPropFormat::V11Lite | StaticPropFormat::V11BlackMesa | StaticPropFormat::V11Csgo => 11,
        }
    }

    pub const fn size(self) -> usize {
        match self {
            StaticPropFormat::V4 => 56,
            StaticPropFormat::V5 => 60,
            StaticPropFormat::V5Ship => 60 + TARGETNAME_LEN,
            StaticPropFormat::V6 => 64,
            StaticPropFormat::V6Bgt => 64 + TARGETNAME_LEN,
            StaticPropFormat::V7L4d | StaticPropFormat::V8 => 68,
            StaticPropFormat::V9 | StaticPropFormat::V10 => 72,
            StaticPropFormat::V10Csgo | StaticPropFormat::V11Lite => 76,
            StaticPropFormat::V11BlackMesa | StaticPropFormat::V11Csgo => 80,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            StaticPropFormat::V4 => "v4",
            StaticPropFormat::V5 => "v5",
            StaticPropFormat::V5Ship => "v5 (The Ship)",
            StaticPropFormat::V6 => "v6",
            StaticPropFormat::V6Bgt => "v6 (Bloody Good Time)",
            StaticPropFormat::V7L4d => "v7 (Left 4 Dead)",
            StaticPropFormat::V8 => "v8",
            StaticPropFormat::V9 => "v9",
            StaticPropFormat::V10 => "v10 (Source 2013)",
            StaticPropFormat::V10Csgo => "v10 (CS:GO)",
            StaticPropFormat::V11Lite => "v11 (Black Mesa 2017)",
            StaticPropFormat::V11BlackMesa => "v11 (Black Mesa)",
            StaticPropFormat::V11Csgo => "v11 (CS:GO)",
        }
    }

    pub fn detect(version: u16, size: usize, preferred: Option<StaticPropFormat>) -> Option<StaticPropFormat> {
        if let Some(p) = preferred
            && p.version() == version
            && p.size() == size
        {
            return Some(p);
        }
        if version == 7 && size == 72 {
            return Some(StaticPropFormat::V10);
        }
        Self::ALL.into_iter().find(|f| f.version() == version && f.size() == size)
    }

    pub fn detect_in(
        version: u16,
        size: usize,
        preferred: Option<StaticPropFormat>,
        first: Option<&[u8]>,
    ) -> Option<StaticPropFormat> {
        let found = Self::detect(version, size, preferred)?;
        if preferred.is_some_and(|p| p == found) || found != StaticPropFormat::V11BlackMesa {
            return Some(found);
        }
        let scale = first.and_then(|r| r.get(76..80)).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]));
        Some(match scale {
            Some(s) if s.is_finite() && s > 1e-3 && s < 1e4 => StaticPropFormat::V11Csgo,
            _ => StaticPropFormat::V11BlackMesa,
        })
    }

    pub const fn has_wide_flags(self) -> bool {
        matches!(self, StaticPropFormat::V10 | StaticPropFormat::V11Lite | StaticPropFormat::V11BlackMesa)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StaticPropRecord {
    pub origin: [f32; 3],
    pub angles: [f32; 3],
    pub prop_type: u16,
    pub first_leaf: u16,
    pub leaf_count: u16,
    pub solid: u8,
    pub flags: u32,
    pub skin: i32,
    pub fade_min: f32,
    pub fade_max: f32,
    pub lighting_origin: [f32; 3],
    pub forced_fade_scale: f32,
    pub min_dx_level: u16,
    pub max_dx_level: u16,
    pub min_cpu_level: u8,
    pub max_cpu_level: u8,
    pub min_gpu_level: u8,
    pub max_gpu_level: u8,
    pub diffuse_modulation: [u8; 4],
    pub disable_x360: bool,
    pub flags_ex: u32,
    pub lightmap_resolution: [u16; 2],
    pub uniform_scale: f32,
    pub targetname: String,
}

impl Default for StaticPropRecord {
    fn default() -> Self {
        Self {
            origin: [0.0; 3],
            angles: [0.0; 3],
            prop_type: 0,
            first_leaf: 0,
            leaf_count: 0,
            solid: 0,
            flags: 0,
            skin: 0,
            fade_min: 0.0,
            fade_max: 0.0,
            lighting_origin: [0.0; 3],
            forced_fade_scale: 1.0,
            min_dx_level: 0,
            max_dx_level: 0,
            min_cpu_level: 0,
            max_cpu_level: 0,
            min_gpu_level: 0,
            max_gpu_level: 0,
            diffuse_modulation: [255; 4],
            disable_x360: false,
            flags_ex: 0,
            lightmap_resolution: [0; 2],
            uniform_scale: 1.0,
            targetname: String::new(),
        }
    }
}

struct Out<'a>(&'a mut Vec<u8>);

impl Out<'_> {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn vec3(&mut self, v: [f32; 3]) {
        for c in v {
            self.f32(c);
        }
    }
    fn name(&mut self, s: &str, len: usize) {
        let mut raw = vec![0u8; len];
        let bytes = s.as_bytes();
        let n = bytes.len().min(len - 1);
        raw[..n].copy_from_slice(&bytes[..n]);
        self.0.extend_from_slice(&raw);
    }
}

struct In<'a> {
    b: &'a [u8],
    at: usize,
}

impl In<'_> {
    fn take(&mut self, n: usize) -> &[u8] {
        let s = &self.b[self.at..self.at + n];
        self.at += n;
        s
    }
    fn u8(&mut self) -> u8 {
        self.take(1)[0]
    }
    fn u16(&mut self) -> u16 {
        let s = self.take(2);
        u16::from_le_bytes([s[0], s[1]])
    }
    fn u32(&mut self) -> u32 {
        let s = self.take(4);
        u32::from_le_bytes([s[0], s[1], s[2], s[3]])
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.u32())
    }
    fn vec3(&mut self) -> [f32; 3] {
        [self.f32(), self.f32(), self.f32()]
    }
    fn name(&mut self, len: usize) -> String {
        let raw = self.take(len);
        let end = raw.iter().position(|&b| b == 0).unwrap_or(len);
        String::from_utf8_lossy(&raw[..end]).into_owned()
    }
}

impl StaticPropRecord {
    pub fn write(&self, format: StaticPropFormat, out: &mut Vec<u8>) {
        use StaticPropFormat as F;
        let start = out.len();
        let mut o = Out(out);
        o.vec3(self.origin);
        o.vec3(self.angles);
        o.u16(self.prop_type);
        o.u16(self.first_leaf);
        o.u16(self.leaf_count);
        o.u8(self.solid);
        o.u8((self.flags & 0xff) as u8);
        o.i32(self.skin);
        o.f32(self.fade_min);
        o.f32(self.fade_max);
        o.vec3(self.lighting_origin);
        if format == F::V4 {
            return;
        }
        o.f32(self.forced_fade_scale);
        let color = u32::from_le_bytes(self.diffuse_modulation);
        match format {
            F::V5 => {}
            F::V5Ship => o.name(&self.targetname, TARGETNAME_LEN),
            F::V8 | F::V9 | F::V10Csgo | F::V11Csgo => {
                o.u8(self.min_cpu_level);
                o.u8(self.max_cpu_level);
                o.u8(self.min_gpu_level);
                o.u8(self.max_gpu_level);
                o.u32(color);
                if format != F::V8 {
                    o.u8(u8::from(self.disable_x360));
                    o.u8(0);
                    o.u8(0);
                    o.u8(0);
                }
                if matches!(format, F::V10Csgo | F::V11Csgo) {
                    o.u32(self.flags_ex);
                }
                if format == F::V11Csgo {
                    o.f32(self.uniform_scale);
                }
            }
            F::V6 | F::V6Bgt | F::V7L4d | F::V10 | F::V11Lite | F::V11BlackMesa => {
                o.u16(self.min_dx_level);
                o.u16(self.max_dx_level);
                match format {
                    F::V6Bgt => o.name(&self.targetname, TARGETNAME_LEN),
                    F::V7L4d => o.u32(color),
                    F::V10 | F::V11Lite | F::V11BlackMesa => {
                        o.u32(self.flags);
                        o.u16(self.lightmap_resolution[0]);
                        o.u16(self.lightmap_resolution[1]);
                        if format != F::V10 {
                            o.u32(color);
                        }
                        if format == F::V11BlackMesa {
                            o.u32(self.flags_ex);
                        }
                    }
                    _ => {}
                }
            }
            F::V4 => {}
        }
        debug_assert_eq!(out.len() - start, format.size(), "{format:?}");
    }

    pub fn read(format: StaticPropFormat, b: &[u8]) -> Option<StaticPropRecord> {
        use StaticPropFormat as F;
        if b.len() < format.size() {
            return None;
        }
        let mut i = In { b, at: 0 };
        let mut r = StaticPropRecord {
            origin: i.vec3(),
            angles: i.vec3(),
            prop_type: i.u16(),
            first_leaf: i.u16(),
            leaf_count: i.u16(),
            solid: i.u8(),
            flags: u32::from(i.u8()),
            skin: i.u32() as i32,
            fade_min: i.f32(),
            fade_max: i.f32(),
            lighting_origin: i.vec3(),
            ..StaticPropRecord::default()
        };
        if format == F::V4 {
            return Some(r);
        }
        r.forced_fade_scale = i.f32();
        match format {
            F::V5 | F::V4 => {}
            F::V5Ship => r.targetname = i.name(TARGETNAME_LEN),
            F::V8 | F::V9 | F::V10Csgo | F::V11Csgo => {
                r.min_cpu_level = i.u8();
                r.max_cpu_level = i.u8();
                r.min_gpu_level = i.u8();
                r.max_gpu_level = i.u8();
                r.diffuse_modulation = i.u32().to_le_bytes();
                if format != F::V8 {
                    r.disable_x360 = i.u8() != 0;
                    i.take(3);
                }
                if matches!(format, F::V10Csgo | F::V11Csgo) {
                    r.flags_ex = i.u32();
                }
                if format == F::V11Csgo {
                    r.uniform_scale = i.f32();
                }
            }
            F::V6 | F::V6Bgt | F::V7L4d | F::V10 | F::V11Lite | F::V11BlackMesa => {
                r.min_dx_level = i.u16();
                r.max_dx_level = i.u16();
                match format {
                    F::V6Bgt => r.targetname = i.name(TARGETNAME_LEN),
                    F::V7L4d => r.diffuse_modulation = i.u32().to_le_bytes(),
                    F::V10 | F::V11Lite | F::V11BlackMesa => {
                        r.flags = i.u32();
                        r.lightmap_resolution = [i.u16(), i.u16()];
                        if format != F::V10 {
                            r.diffuse_modulation = i.u32().to_le_bytes();
                        }
                        if format == F::V11BlackMesa {
                            r.flags_ex = i.u32();
                        }
                    }
                    _ => {}
                }
            }
        }
        Some(r)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct StaticPropLump {
    pub names: Vec<String>,
    pub leaves: Vec<u16>,
    pub props: Vec<StaticPropRecord>,
}

fn i32_at(b: &[u8], at: usize) -> Option<i32> {
    Some(i32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn count_at(b: &[u8], at: usize, unit: usize) -> Result<usize> {
    let n = i32_at(b, at).ok_or_else(|| BspError::corrupt("the static prop lump is cut short"))?;
    let n = usize::try_from(n).map_err(|_| BspError::corrupt("negative count in the static prop lump"))?;
    if n.saturating_mul(unit) > b.len().saturating_sub(at + 4) {
        return Err(BspError::corrupt("the static prop lump is cut short"));
    }
    Ok(n)
}

impl StaticPropLump {
    pub fn parse(
        data: &[u8],
        version: u16,
        preferred: Option<StaticPropFormat>,
    ) -> Result<(StaticPropLump, Option<StaticPropFormat>)> {
        if data.is_empty() {
            return Ok((StaticPropLump::default(), None));
        }
        let n = count_at(data, 0, NAME_LEN)?;
        let mut at = 4;
        let mut names = Vec::with_capacity(n);
        for _ in 0..n {
            let mut i = In { b: data, at };
            names.push(i.name(NAME_LEN).replace('\\', "/"));
            at += NAME_LEN;
        }
        let leaf_count = count_at(data, at, 2)?;
        at += 4;
        let leaves = (0..leaf_count).map(|k| u16::from_le_bytes([data[at + 2 * k], data[at + 2 * k + 1]])).collect();
        at += 2 * leaf_count;
        let count = count_at(data, at, 1)?;
        at += 4;
        if count == 0 {
            return Ok((
                StaticPropLump { names, leaves, props: Vec::new() },
                preferred.filter(|p| p.version() == version),
            ));
        }
        let rest = data.len() - at;
        let size = rest / count;
        let format =
            StaticPropFormat::detect_in(version, size, preferred, data.get(at..at + size)).ok_or_else(|| {
                BspError::corrupt(format!(
                    "static props: version {version} with records of {size} bytes is not a known layout"
                ))
            })?;
        let props = (0..count)
            .map(|k| StaticPropRecord::read(format, &data[at + k * size..]))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| BspError::corrupt("the static prop lump is cut short"))?;
        Ok((StaticPropLump { names, leaves, props }, Some(format)))
    }

    pub fn to_bytes(&self, format: StaticPropFormat) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            12 + self.names.len() * NAME_LEN + self.leaves.len() * 2 + self.props.len() * format.size(),
        );
        out.extend_from_slice(&(self.names.len() as i32).to_le_bytes());
        for n in &self.names {
            Out(&mut out).name(n, NAME_LEN);
        }
        out.extend_from_slice(&(self.leaves.len() as i32).to_le_bytes());
        for l in &self.leaves {
            out.extend_from_slice(&l.to_le_bytes());
        }
        out.extend_from_slice(&(self.props.len() as i32).to_le_bytes());
        for p in &self.props {
            p.write(format, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> StaticPropRecord {
        StaticPropRecord {
            origin: [1.0, -2.0, 3.5],
            angles: [0.0, 90.0, -45.0],
            prop_type: 3,
            first_leaf: 7,
            leaf_count: 2,
            solid: 6,
            flags: 0x1 | 0x10 | FLAG_NO_PER_TEXEL_LIGHTING,
            skin: 2,
            fade_min: 100.0,
            fade_max: 900.0,
            lighting_origin: [4.0, 5.0, 6.0],
            forced_fade_scale: 0.5,
            min_dx_level: 80,
            max_dx_level: 95,
            min_cpu_level: 1,
            max_cpu_level: 3,
            min_gpu_level: 2,
            max_gpu_level: 3,
            diffuse_modulation: [10, 20, 30, 40],
            disable_x360: true,
            flags_ex: 5,
            lightmap_resolution: [32, 16],
            uniform_scale: 1.5,
            targetname: "boat".to_owned(),
        }
    }

    #[test]
    fn every_format_writes_its_size_and_reads_back() {
        let r = sample();
        for f in StaticPropFormat::ALL {
            let mut out = Vec::new();
            r.write(f, &mut out);
            assert_eq!(out.len(), f.size(), "{f:?}");
            let back = StaticPropRecord::read(f, &out).expect("read");
            assert_eq!(back.origin, r.origin, "{f:?}");
            assert_eq!(back.lighting_origin, r.lighting_origin, "{f:?}");
            assert_eq!(
                (back.prop_type, back.first_leaf, back.leaf_count, back.solid, back.skin),
                (3, 7, 2, 6, 2),
                "{f:?}"
            );
            assert_eq!(back.flags & 0xff, 0x11, "{f:?}");
            assert_eq!(StaticPropFormat::detect(f.version(), f.size(), Some(f)), Some(f));
        }
    }

    #[test]
    fn field_offsets_follow_the_headers() {
        let r = sample();
        let bytes = |f: StaticPropFormat| {
            let mut out = Vec::new();
            r.write(f, &mut out);
            out
        };
        let u16_at = |b: &[u8], o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
        let u32_at = |b: &[u8], o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        let f32_at = |b: &[u8], o: usize| f32::from_bits(u32_at(b, o));
        for f in StaticPropFormat::ALL {
            let b = bytes(f);
            assert_eq!(f32_at(&b, 0), 1.0);
            assert_eq!(f32_at(&b, 16), 90.0);
            assert_eq!(u16_at(&b, 24), 3);
            assert_eq!(u16_at(&b, 26), 7);
            assert_eq!(u16_at(&b, 28), 2);
            assert_eq!(b[30], 6);
            assert_eq!(b[31], 0x11);
            assert_eq!(u32_at(&b, 32), 2);
            assert_eq!(f32_at(&b, 36), 100.0);
            assert_eq!(f32_at(&b, 40), 900.0);
            assert_eq!(f32_at(&b, 44), 4.0);
            if f != StaticPropFormat::V4 {
                assert_eq!(f32_at(&b, 56), 0.5, "{f:?}: forced fade scale");
            }
        }
        let v6 = bytes(StaticPropFormat::V6);
        assert_eq!((u16_at(&v6, 60), u16_at(&v6, 62)), (80, 95));
        let v10 = bytes(StaticPropFormat::V10);
        assert_eq!(u32_at(&v10, 64), 0x111);
        assert_eq!((u16_at(&v10, 68), u16_at(&v10, 70)), (32, 16));
        let v8 = bytes(StaticPropFormat::V8);
        assert_eq!(&v8[60..64], &[1, 3, 2, 3]);
        assert_eq!(&v8[64..68], &[10, 20, 30, 40]);
        let v9 = bytes(StaticPropFormat::V9);
        assert_eq!(&v9[68..72], &[1, 0, 0, 0]);
        let csgo = bytes(StaticPropFormat::V11Csgo);
        assert_eq!(u32_at(&csgo, 72), 5);
        assert_eq!(f32_at(&csgo, 76), 1.5);
        let bm = bytes(StaticPropFormat::V11BlackMesa);
        assert_eq!(u32_at(&bm, 64), 0x111);
        assert_eq!(&bm[72..76], &[10, 20, 30, 40]);
        assert_eq!(u32_at(&bm, 76), 5);
        let l4d = bytes(StaticPropFormat::V7L4d);
        assert_eq!(&l4d[64..68], &[10, 20, 30, 40]);
        let ship = bytes(StaticPropFormat::V5Ship);
        assert_eq!(&ship[60..65], b"boat\0");
        let bgt = bytes(StaticPropFormat::V6Bgt);
        assert_eq!(&bgt[64..69], b"boat\0");
    }

    #[test]
    fn same_version_layouts_are_told_apart() {
        assert_eq!(StaticPropFormat::detect(10, 72, None), Some(StaticPropFormat::V10));
        assert_eq!(StaticPropFormat::detect(10, 76, None), Some(StaticPropFormat::V10Csgo));
        assert_eq!(StaticPropFormat::detect(11, 80, None), Some(StaticPropFormat::V11BlackMesa));
        assert_eq!(StaticPropFormat::detect(11, 80, Some(StaticPropFormat::V11Csgo)), Some(StaticPropFormat::V11Csgo));
        assert_eq!(StaticPropFormat::detect(11, 76, None), Some(StaticPropFormat::V11Lite));
        assert_eq!(StaticPropFormat::detect(7, 68, None), Some(StaticPropFormat::V7L4d));
        assert_eq!(StaticPropFormat::detect(7, 72, None), Some(StaticPropFormat::V10));
        assert_eq!(StaticPropFormat::detect(5, 188, None), Some(StaticPropFormat::V5Ship));
        assert_eq!(StaticPropFormat::detect(9, 70, None), None);
        let lump = StaticPropLump {
            names: vec!["m".to_owned()],
            leaves: vec![0],
            props: vec![StaticPropRecord { flags_ex: 4, uniform_scale: 1.0, ..StaticPropRecord::default() }],
        };
        for f in [StaticPropFormat::V11Csgo, StaticPropFormat::V11BlackMesa] {
            let (_, found) = StaticPropLump::parse(&lump.to_bytes(f), 11, None).expect("parse");
            assert_eq!(found, Some(f));
        }
    }

    #[test]
    fn lumps_round_trip_in_every_format() {
        let lump = StaticPropLump {
            names: vec!["models/a.mdl".to_owned(), "models/props/b.mdl".to_owned()],
            leaves: vec![1, 2, 9],
            props: vec![sample(), StaticPropRecord { prop_type: 1, ..sample() }],
        };
        for f in StaticPropFormat::ALL {
            let bytes = lump.to_bytes(f);
            assert_eq!(bytes.len(), 4 + 2 * 128 + 4 + 6 + 4 + 2 * f.size());
            let (back, found) = StaticPropLump::parse(&bytes, f.version(), Some(f)).expect("parse");
            assert_eq!(found, Some(f));
            assert_eq!(back.names, lump.names);
            assert_eq!(back.leaves, lump.leaves);
            assert_eq!(back.props.len(), 2);
            assert_eq!(back.props[1].prop_type, 1);
        }
        assert!(StaticPropLump::parse(&[1, 0, 0, 0], 10, None).is_err());
        assert_eq!(StaticPropLump::parse(&[], 10, None).expect("empty").0, StaticPropLump::default());
    }
}
