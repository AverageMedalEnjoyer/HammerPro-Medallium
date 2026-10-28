use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const MAX_RECORD_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstanceRecord {
    pub pid: u32,
    pub port: u16,
    pub token: String,
    pub map_title: String,
    pub started: u64,
    pub last_focused: u64,
    #[serde(default)]
    pub version: String,
}

pub fn instances_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(crate::DIR_ENV).filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    dirs::data_local_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(std::env::temp_dir)
        .join("HammerPro")
        .join("mcp")
        .join("instances")
}

pub fn record_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{pid}.json"))
}

pub fn now_millis() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

pub fn write_record(dir: &Path, record: &InstanceRecord) -> io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let text = serde_json::to_string_pretty(record).map_err(io::Error::other)?;
    let path = record_path(dir, record.pid);
    let tmp = dir.join(format!("{}.json.tmp", record.pid));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub fn remove_record(dir: &Path, pid: u32) {
    let path = record_path(dir, pid);
    if let Err(e) = std::fs::remove_file(&path)
        && e.kind() != io::ErrorKind::NotFound
    {
        log::warn!("cannot remove {}: {e}", path.display());
    }
}

pub fn read_record(path: &Path) -> Option<InstanceRecord> {
    let pid: u32 = path.file_stem()?.to_str()?.parse().ok()?;
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_RECORD_BYTES {
        return None;
    }
    let text = std::fs::read_to_string(path).ok()?;
    let record: InstanceRecord = serde_json::from_str(&text).ok()?;
    let token_ok = record.token.len() == 64 && record.token.bytes().all(|b| b.is_ascii_hexdigit());
    (record.pid == pid && record.port != 0 && token_ok).then_some(record)
}

pub fn live_instances(dir: &Path) -> Vec<InstanceRecord> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Some(record) = read_record(&path) else { continue };
        if pid_alive(record.pid) {
            out.push(record);
        } else {
            log::info!("removing the MCP record of process {} (not running)", record.pid);
            let _ = std::fs::remove_file(&path);
        }
    }
    out.sort_by(|a, b| b.last_focused.cmp(&a.last_focused).then(a.pid.cmp(&b.pid)));
    out
}

#[cfg(windows)]
pub fn pid_alive(pid: u32) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, GetLastError};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    const STILL_ACTIVE: u32 = 259;
    if pid == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return GetLastError() == ERROR_ACCESS_DENIED;
        }
        let mut code = 0u32;
        let ok = GetExitCodeProcess(handle, &mut code) != 0;
        CloseHandle(handle);
        ok && code == STILL_ACTIVE
    }
}

#[cfg(not(windows))]
pub fn pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    let proc = Path::new("/proc");
    if proc.is_dir() { proc.join(pid.to_string()).exists() } else { true }
}

#[cfg(test)]
pub(crate) fn temp_dir(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "hp_mcp_test_{}_{}_{}",
        name,
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(pid: u32, focused: u64) -> InstanceRecord {
        InstanceRecord {
            pid,
            port: 4000,
            token: "ab".repeat(32),
            map_title: "gm_test.vmf".into(),
            started: 1,
            last_focused: focused,
            version: crate::VERSION.into(),
        }
    }

    #[test]
    fn records_round_trip_and_dead_ones_go() {
        let dir = temp_dir("registry");
        let me = std::process::id();
        write_record(&dir, &record(me, 10)).unwrap();
        assert_eq!(read_record(&record_path(&dir, me)), Some(record(me, 10)));
        let dead = 4_000_000_001;
        write_record(&dir, &record(dead, 99)).unwrap();
        std::fs::write(dir.join("123.json"), "{oops").unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let mut wrong = record(me, 5);
        wrong.pid = 77;
        std::fs::write(dir.join(format!("{}.json", me + 1)), serde_json::to_string(&wrong).unwrap()).unwrap();
        let live = live_instances(&dir);
        assert_eq!(live, vec![record(me, 10)]);
        assert!(!record_path(&dir, dead).exists(), "the record of a dead process is removed");
        assert!(dir.join("123.json").exists(), "files that are not records are left alone");
        remove_record(&dir, me);
        remove_record(&dir, me);
        assert!(live_instances(&dir).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_tokens_are_refused() {
        let dir = temp_dir("registry_token");
        let me = std::process::id();
        let mut r = record(me, 1);
        r.token = "short".into();
        write_record(&dir, &r).unwrap();
        assert!(read_record(&record_path(&dir, me)).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn this_process_is_alive() {
        assert!(pid_alive(std::process::id()));
        assert!(!pid_alive(0));
    }
}
