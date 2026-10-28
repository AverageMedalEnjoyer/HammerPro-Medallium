use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub(crate) const MANIFEST_NAME: &str = ".hammerpro_release.toml";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Written {
    pub source_size: u64,
    pub source_mtime: u64,
    pub size: u64,
    pub mtime: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ReleaseManifest {
    pub version: u32,
    pub files: BTreeMap<String, Written>,
    pub gma_digest: Option<u64>,
    pub bsp_digest: Option<u64>,
    pub bsp_written: Option<(u64, u64)>,
}

impl ReleaseManifest {
    pub(crate) fn load(out_root: &Path) -> ReleaseManifest {
        let path = out_root.join(MANIFEST_NAME);
        super::files::read_limited(&path, 64 << 20)
            .and_then(|b| String::from_utf8(b).ok())
            .and_then(|t| toml::from_str::<ReleaseManifest>(&t).ok())
            .unwrap_or_default()
    }

    pub(crate) fn save(&self, out_root: &Path) -> std::io::Result<()> {
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        super::files::write_atomic(&out_root.join(MANIFEST_NAME), text.as_bytes())
    }

    pub(crate) fn up_to_date(&self, vpath: &str, source: (u64, u64), dest: Option<(u64, u64)>) -> bool {
        let Some(w) = self.files.get(vpath) else { return false };
        let Some(dest) = dest else { return false };
        (w.source_size, w.source_mtime) == source && (w.size, w.mtime) == dest
    }
}

pub(crate) fn digest<'a>(items: impl Iterator<Item = (&'a str, u64, u64)>) -> u64 {
    let mut list: Vec<(&str, u64, u64)> = items.collect();
    list.sort_unstable();
    let mut a = fnv::Hasher::default();
    for (p, s, m) in list {
        a.update(p.as_bytes());
        a.update(&s.to_le_bytes());
        a.update(&m.to_le_bytes());
    }
    a.finish()
}

mod fnv {
    pub(crate) struct Hasher(u64);

    impl Default for Hasher {
        fn default() -> Self {
            Hasher(0xcbf2_9ce4_8422_2325)
        }
    }

    impl Hasher {
        pub(crate) fn update(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 ^= u64::from(b);
                self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
            }
        }

        pub(crate) fn finish(&self) -> u64 {
            self.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_checks() {
        let dir = crate::test_scratch("release_manifest");
        assert_eq!(ReleaseManifest::load(&dir), ReleaseManifest::default());
        let mut m = ReleaseManifest { version: 1, ..ReleaseManifest::default() };
        m.files.insert("materials/a b/c.vmt".into(), Written { source_size: 1, source_mtime: 2, size: 1, mtime: 3 });
        m.gma_digest = Some(42);
        m.save(&dir).unwrap();
        let back = ReleaseManifest::load(&dir);
        assert_eq!(back, m);
        assert!(back.up_to_date("materials/a b/c.vmt", (1, 2), Some((1, 3))));
        assert!(!back.up_to_date("materials/a b/c.vmt", (1, 9), Some((1, 3))));
        assert!(!back.up_to_date("materials/a b/c.vmt", (1, 2), None));
        std::fs::write(dir.join(MANIFEST_NAME), "garbage = [").unwrap();
        assert_eq!(ReleaseManifest::load(&dir), ReleaseManifest::default());
        let d1 = digest([("a", 1, 2), ("b", 3, 4)].into_iter());
        let d2 = digest([("b", 3, 4), ("a", 1, 2)].into_iter());
        assert_eq!(d1, d2);
        assert_ne!(d1, digest([("a", 1, 2)].into_iter()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
