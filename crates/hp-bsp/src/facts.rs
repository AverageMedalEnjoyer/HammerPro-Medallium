use crate::bsp::Bsp;
use crate::error::Result;
use crate::gamelump::id;
use crate::header::lump;
use crate::limits::{self, MapLimit};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LimitUsage {
    pub name: &'static str,
    pub used: u64,
    pub max: u64,
}

impl LimitUsage {
    pub fn fraction(&self) -> f64 {
        if self.max == 0 { 0.0 } else { self.used as f64 / self.max as f64 }
    }
}

const RECORD_LIMITS: &[(usize, MapLimit)] = &[
    (1, limits::PLANES),
    (3, limits::VERTEXES),
    (5, limits::NODES),
    (6, limits::TEXINFO),
    (lump::FACES, limits::FACES),
    (12, limits::EDGES),
    (13, limits::SURFEDGES),
    (lump::MODELS, limits::MODELS),
    (18, limits::BRUSHES),
    (19, limits::BRUSHSIDES),
    (lump::TEXDATA, limits::TEXDATA),
    (lump::OVERLAYS, limits::OVERLAYS),
];

const WALKABLE_LUMPS: &[usize] = &[1, 5, 10, 14, 18, 19, 26, 29, 33, 48];

impl Bsp {
    pub fn limit_usage(&self) -> Vec<LimitUsage> {
        let mut out: Vec<LimitUsage> = RECORD_LIMITS
            .iter()
            .filter_map(|&(l, limit)| {
                let info = self.lump_info(l).ok()?;
                let len = info.uncompressed_len().map_or(u64::from(info.len), u64::from);
                Some(LimitUsage { name: limit.name, used: len / limit.record, max: limit.max() })
            })
            .collect();
        if let Ok(ents) = self.entities() {
            out.push(LimitUsage { name: "entities", used: ents.len() as u64, max: limits::ENTITIES.max() });
        }
        for (l, name) in [(lump::LIGHTING, "lightmaps"), (lump::LIGHTING_HDR, "lightmaps_hdr")] {
            if let Ok(info) = self.lump_info(l) {
                let len = info.uncompressed_len().map_or(u64::from(info.len), u64::from);
                out.push(LimitUsage { name, used: len, max: limits::LIGHTING.max() });
            }
        }
        out
    }

    pub fn walkable_fingerprint(&self) -> Result<u64> {
        let mut a = crc32fast::Hasher::new();
        let mut b = crc32fast::Hasher::new_with_initial(0x9E37_79B9);
        let mut feed = |data: &[u8]| {
            a.update(&(data.len() as u64).to_le_bytes());
            a.update(data);
            b.update(data);
            b.update(&(data.len() as u64).to_le_bytes());
        };
        for &l in WALKABLE_LUMPS {
            feed(&self.lump(l)?);
        }
        let props = self.game_lumps()?;
        match props.get(id::STATIC_PROPS) {
            Some(g) => feed(&g.data()?),
            None => feed(&[]),
        }
        Ok((u64::from(a.finalize()) << 32) | u64::from(b.finalize()))
    }
}

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn fingerprint_ignores_lighting_and_pakfile() {
        let base = |planes: u8| {
            synthetic_bsp(
                20,
                &[
                    (1, vec![planes; 40]),
                    (lump::LIGHTING, vec![1; 16]),
                    (lump::PAKFILE, PakWriter::new().finish(PakOptions::default()).unwrap()),
                ],
            )
        };
        let a = Bsp::from_vec(base(1)).unwrap();
        let fp = a.walkable_fingerprint().unwrap();
        let mut edit = Bsp::from_vec(base(1)).unwrap().edit();
        edit.replace_lump(lump::LIGHTING, vec![9; 64]).unwrap();
        let mut w = PakWriter::new();
        w.add("materials/x.vmt", b"x".to_vec()).unwrap();
        edit.set_pakfile(w.finish(PakOptions::default()).unwrap()).unwrap();
        let b = Bsp::from_vec(edit.to_vec().unwrap()).unwrap();
        assert_eq!(b.walkable_fingerprint().unwrap(), fp);
        let c = Bsp::from_vec(base(2)).unwrap();
        assert_ne!(c.walkable_fingerprint().unwrap(), fp);
        let usage = a.limit_usage();
        let planes = usage.iter().find(|u| u.name == "planes").unwrap();
        assert_eq!(planes.used, 2);
        assert!(planes.fraction() < 0.01);
    }
}
