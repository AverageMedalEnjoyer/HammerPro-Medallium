use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde::de::DeserializeOwned;

pub const META_SECTION: &str = "sidecar";

pub const FORMAT_VERSION: i64 = 1;

pub trait SidecarSection: Serialize + DeserializeOwned + Default {
    const NAME: &'static str;
}

fn path_hash(path: &Path) -> u32 {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let text = absolute.to_string_lossy().replace('\\', "/").to_lowercase();
    let mut h: u32 = 0x811c_9dc5;
    for b in text.bytes() {
        h ^= u32::from(b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

pub fn map_key(path: Option<&Path>) -> String {
    match path {
        Some(p) => {
            let stem: String = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
                .chars()
                .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
                .collect();
            format!("{}_{:08x}", if stem.is_empty() { "map" } else { &stem }, path_hash(p))
        }
        None => "untitled".to_owned(),
    }
}

static NEXT_SERIAL: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct Sidecar {
    dir: Option<PathBuf>,
    map: Option<PathBuf>,
    table: toml::Table,
    dirty: bool,
    serial: u64,
}

impl Default for Sidecar {
    fn default() -> Self {
        Self::in_memory()
    }
}

impl Sidecar {
    pub fn default_dir() -> PathBuf {
        crate::settings::Settings::cache_dir().join("maps")
    }

    pub fn file_in(dir: &Path, map: &Path) -> PathBuf {
        dir.join(format!("{}.toml", map_key(Some(map))))
    }

    pub fn in_memory() -> Self {
        Self::open(None, None)
    }

    pub fn open(dir: Option<&Path>, map: Option<&Path>) -> Self {
        let table = match (dir, map) {
            (Some(dir), Some(map)) => read_table(&Self::file_in(dir, map)),
            _ => toml::Table::new(),
        };
        Self {
            dir: dir.map(Path::to_path_buf),
            map: map.map(Path::to_path_buf),
            table,
            dirty: false,
            serial: NEXT_SERIAL.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    pub fn map(&self) -> Option<&Path> {
        self.map.as_deref()
    }

    pub fn file(&self) -> Option<PathBuf> {
        Some(Self::file_in(self.dir.as_deref()?, self.map.as_deref()?))
    }

    pub fn serial(&self) -> u64 {
        self.serial
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn contains(&self, name: &str) -> bool {
        self.table.contains_key(name)
    }

    pub fn sections(&self) -> impl Iterator<Item = &str> {
        self.table.keys().map(String::as_str).filter(|k| *k != META_SECTION)
    }

    pub fn get<T: SidecarSection>(&self) -> T {
        self.get_named(T::NAME)
    }

    pub fn set<T: SidecarSection>(&mut self, value: &T) {
        self.set_named(T::NAME, value);
    }

    pub fn get_named<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        let Some(value) = self.table.get(name) else { return T::default() };
        match value.clone().try_into() {
            Ok(v) => v,
            Err(e) => {
                log::warn!("sidecar section {name} is invalid, using defaults: {e}");
                T::default()
            }
        }
    }

    pub fn set_named<T: Serialize>(&mut self, name: &str, value: &T) {
        if name == META_SECTION {
            log::warn!("sidecar section name {name} is reserved");
            return;
        }
        match toml::Value::try_from(value) {
            Ok(v) => {
                if self.table.get(name) != Some(&v) {
                    self.table.insert(name.to_owned(), v);
                    self.dirty = true;
                }
            }
            Err(e) => log::warn!("sidecar section {name} cannot be stored: {e}"),
        }
    }

    pub fn remove(&mut self, name: &str) {
        if self.table.remove(name).is_some() {
            self.dirty = true;
        }
    }

    pub fn flush(&mut self) -> std::io::Result<bool> {
        if !self.dirty {
            return Ok(false);
        }
        let Some(file) = self.file() else { return Ok(false) };
        self.write(&file)?;
        self.dirty = false;
        Ok(true)
    }

    pub fn save_as(&mut self, map: &Path) -> std::io::Result<bool> {
        let moved = self.map.as_deref() != Some(map);
        self.map = Some(map.to_path_buf());
        if moved && self.sections().next().is_some() {
            self.dirty = true;
        }
        self.flush()
    }

    fn write(&mut self, file: &Path) -> std::io::Result<()> {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut meta = toml::Table::new();
        meta.insert("version".to_owned(), toml::Value::Integer(FORMAT_VERSION));
        if let Some(map) = &self.map {
            meta.insert("map".to_owned(), toml::Value::String(map.to_string_lossy().into_owned()));
        }
        self.table.insert(META_SECTION.to_owned(), toml::Value::Table(meta));
        let text = toml::to_string_pretty(&self.table).map_err(std::io::Error::other)?;
        let tmp = file.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, file)
    }
}

fn read_table(file: &Path) -> toml::Table {
    let text = match std::fs::read_to_string(file) {
        Ok(text) => text,
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                log::warn!("cannot read the map sidecar {}: {e}", file.display());
            }
            return toml::Table::new();
        }
    };
    match toml::from_str::<toml::Table>(&text) {
        Ok(table) => table,
        Err(e) => {
            log::warn!("the map sidecar {} is corrupt, using defaults: {e}", file.display());
            toml::Table::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    use super::*;

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    struct ViewMemory {
        hidden: Vec<u32>,
        tool: String,
    }

    impl SidecarSection for ViewMemory {
        const NAME: &'static str = "test_view";
    }

    #[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Fingerprint {
        hash: u64,
    }

    impl SidecarSection for Fingerprint {
        const NAME: &'static str = "test_compile";
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hp_app_sidecar_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn keys_identify_the_full_path() {
        let a = map_key(Some(Path::new("C:/maps/ttt.vmf")));
        let b = map_key(Some(Path::new("C:/maps/ttt_traps.vmf")));
        let c = map_key(Some(Path::new("D:/other/ttt.vmf")));
        assert!(a.starts_with("ttt_") && b.starts_with("ttt_traps_"));
        assert_ne!(a, c);
        assert_eq!(a, map_key(Some(Path::new("c:\\MAPS\\ttt.vmf"))));
        assert_eq!(map_key(None), "untitled");
    }

    #[test]
    fn sections_round_trip_through_the_file() {
        let dir = scratch("round_trip");
        let map = dir.join("room.vmf");
        let mut car = Sidecar::open(Some(&dir), Some(&map));
        assert_eq!(car.get::<ViewMemory>(), ViewMemory::default());
        let view = ViewMemory { hidden: vec![3, 7], tool: "Block".into() };
        car.set(&view);
        car.set(&Fingerprint { hash: 42 });
        assert!(car.is_dirty());
        assert!(car.flush().unwrap());
        assert!(!car.flush().unwrap(), "nothing new to write");
        let back = Sidecar::open(Some(&dir), Some(&map));
        assert_eq!(back.get::<ViewMemory>(), view);
        assert_eq!(back.get::<Fingerprint>().hash, 42);
        assert_ne!(back.serial(), car.serial());
        let mut names: Vec<&str> = back.sections().collect();
        names.sort();
        assert_eq!(names, ["test_compile", "test_view"]);
        let text = std::fs::read_to_string(back.file().unwrap()).unwrap();
        assert!(text.contains("[sidecar]") && text.contains("version = 1"), "{text}");
        let mut again = back;
        again.set(&view);
        assert!(!again.is_dirty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_files_and_sections_read_as_defaults() {
        let dir = scratch("corrupt");
        let map = dir.join("room.vmf");
        std::fs::write(Sidecar::file_in(&dir, &map), "this is [ not toml").unwrap();
        let car = Sidecar::open(Some(&dir), Some(&map));
        assert_eq!(car.get::<ViewMemory>(), ViewMemory::default());
        std::fs::write(Sidecar::file_in(&dir, &map), "[test_compile]\nhash = \"not a number\"\n").unwrap();
        let car = Sidecar::open(Some(&dir), Some(&map));
        assert_eq!(car.get::<Fingerprint>(), Fingerprint::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_as_moves_the_sections_to_the_new_map() {
        let dir = scratch("save_as");
        let mut car = Sidecar::open(Some(&dir), None);
        car.set(&Fingerprint { hash: 7 });
        assert!(!car.flush().unwrap(), "no path yet: stays in memory");
        let map = dir.join("new.vmf");
        assert!(car.save_as(&map).unwrap());
        assert_eq!(Sidecar::open(Some(&dir), Some(&map)).get::<Fingerprint>().hash, 7);
        let copy = dir.join("copy.vmf");
        assert!(car.save_as(&copy).unwrap(), "save as writes the sections under the new name");
        assert_eq!(Sidecar::open(Some(&dir), Some(&copy)).get::<Fingerprint>().hash, 7);
        assert!(!car.save_as(&copy).unwrap(), "saving again without changes writes nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memory_only_sidecars_never_write() {
        let mut car = Sidecar::in_memory();
        car.set(&Fingerprint { hash: 1 });
        assert!(!car.flush().unwrap());
        assert!(!car.save_as(Path::new("C:/maps/x.vmf")).unwrap());
        assert_eq!(car.get::<Fingerprint>().hash, 1);
        car.remove(Fingerprint::NAME);
        assert!(!car.contains(Fingerprint::NAME));
        car.set_named(META_SECTION, &1);
        assert!(!car.contains(META_SECTION), "reserved name");
    }
}
