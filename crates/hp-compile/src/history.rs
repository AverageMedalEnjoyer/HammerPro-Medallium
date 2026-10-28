use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::plan::{StepKind, StepModes};

const READ_LIMIT: u64 = 256 * 1024;
const TRIM_ABOVE: u64 = 512 * 1024;
const KEEP: usize = 500;
pub const SLOW_FACTOR: f64 = 3.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StepTime {
    pub kind: StepKind,
    #[serde(default)]
    pub mode: String,
    pub ms: u64,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct HistoryEntry {
    pub when: u64,
    pub preset: String,
    pub modes: StepModes,
    pub auto: bool,
    pub steps: Vec<StepTime>,
    pub total_ms: u64,
    pub success: bool,
    pub leaked: bool,
    pub portalclusters: Option<u32>,
    pub numportals: Option<u32>,
    pub background: bool,
}

pub fn append(path: &Path, entry: &HistoryEntry) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let line = serde_json::to_string(entry).map_err(std::io::Error::other)?;
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(path)?;
    f.write_all(line.as_bytes())?;
    f.write_all(b"\n")?;
    drop(f);
    if std::fs::metadata(path).map(|m| m.len()).unwrap_or(0) > TRIM_ABOVE {
        let entries = load(path);
        let keep = entries.len().saturating_sub(KEEP);
        let mut text = String::new();
        for e in entries.iter().skip(keep) {
            if let Ok(l) = serde_json::to_string(e) {
                text.push_str(&l);
                text.push('\n');
            }
        }
        let tmp = path.with_extension("jsonl.hptmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, path)?;
    }
    Ok(())
}

pub fn load(path: &Path) -> Vec<HistoryEntry> {
    let Ok(mut f) = std::fs::File::open(path) else { return Vec::new() };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    let from = len.saturating_sub(READ_LIMIT);
    if f.seek(SeekFrom::Start(from)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if f.take(READ_LIMIT).read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.lines();
    if from > 0 {
        lines.next();
    }
    lines.filter_map(|l| serde_json::from_str::<HistoryEntry>(l.trim()).ok()).collect()
}

fn median(mut v: Vec<u64>) -> Option<u64> {
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    v.get(v.len() / 2).copied()
}

pub fn median_ms(entries: &[HistoryEntry], kind: StepKind, mode: &str) -> Option<u64> {
    median(
        entries
            .iter()
            .flat_map(|e| e.steps.iter())
            .filter(|s| s.ok && s.kind == kind && s.mode == mode)
            .map(|s| s.ms)
            .collect(),
    )
}

pub fn slow_steps(earlier: &[HistoryEntry], latest: &HistoryEntry) -> Vec<(StepKind, String, u64, u64)> {
    latest
        .steps
        .iter()
        .filter(|s| s.ok && s.ms >= 2000)
        .filter_map(|s| {
            let runs: Vec<u64> = earlier
                .iter()
                .flat_map(|e| e.steps.iter())
                .filter(|x| x.ok && x.kind == s.kind && x.mode == s.mode)
                .map(|x| x.ms)
                .collect();
            if runs.len() < 3 {
                return None;
            }
            let m = median(runs)?;
            (s.ms as f64 >= SLOW_FACTOR * m.max(1) as f64).then(|| (s.kind, s.mode.clone(), s.ms, m))
        })
        .collect()
}

pub fn estimate_ms(entries: &[HistoryEntry], steps: &[(StepKind, &str)]) -> Option<u64> {
    let mut total = 0u64;
    for (kind, mode) in steps {
        total = total.saturating_add(median_ms(entries, *kind, mode)?);
    }
    Some(total)
}

pub fn parse_portal_stat(line: &str) -> Option<(&'static str, u32)> {
    let mut parts = line.split_ascii_whitespace();
    let n: u32 = parts.next()?.parse().ok()?;
    let word = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if word.eq_ignore_ascii_case("portalclusters") {
        Some(("portalclusters", n))
    } else if word.eq_ignore_ascii_case("numportals") {
        Some(("numportals", n))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(rad_ms: u64) -> HistoryEntry {
        HistoryEntry {
            when: 1,
            preset: "Normal".into(),
            steps: vec![
                StepTime { kind: StepKind::Bsp, mode: "full".into(), ms: 1000, ok: true },
                StepTime { kind: StepKind::Rad, mode: "full".into(), ms: rad_ms, ok: true },
            ],
            total_ms: 1000 + rad_ms,
            success: true,
            ..HistoryEntry::default()
        }
    }

    #[test]
    fn append_load_and_statistics() {
        let dir = crate::test_util::scratch("history");
        let path = dir.join("gm_x_123.jsonl");
        assert!(load(&path).is_empty());
        for ms in [10_000, 11_000, 9_000, 10_500] {
            append(&path, &entry(ms)).unwrap();
        }
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"not json\n{\"steps\": 5}\n").unwrap();
        drop(f);
        let all = load(&path);
        assert_eq!(all.len(), 4);
        assert_eq!(median_ms(&all, StepKind::Rad, "full"), Some(10_500));
        assert_eq!(median_ms(&all, StepKind::Rad, "draft"), None);
        assert_eq!(estimate_ms(&all, &[(StepKind::Bsp, "full"), (StepKind::Rad, "full")]), Some(11_500));
        assert_eq!(estimate_ms(&all, &[(StepKind::Rad, "draft")]), None);
        let slow = slow_steps(&all, &entry(40_000));
        assert_eq!(slow, vec![(StepKind::Rad, "full".to_owned(), 40_000, 10_500)]);
        assert!(slow_steps(&all, &entry(12_000)).is_empty());
        assert!(slow_steps(&all[..2], &entry(40_000)).is_empty(), "too few runs to tell");
    }

    #[test]
    fn trimmed_when_large() {
        let dir = crate::test_util::scratch("history_trim");
        let path = dir.join("big.jsonl");
        let mut big = entry(1);
        big.preset = "x".repeat(2000);
        for _ in 0..300 {
            append(&path, &big).unwrap();
        }
        assert!(std::fs::metadata(&path).unwrap().len() <= TRIM_ABOVE + 4096);
        let all = load(&path);
        assert!(!all.is_empty() && all.len() <= KEEP);
    }

    #[test]
    fn portal_stats() {
        assert_eq!(parse_portal_stat("   72 portalclusters"), Some(("portalclusters", 72)));
        assert_eq!(parse_portal_stat("  1204 numportals"), Some(("numportals", 1204)));
        assert_eq!(parse_portal_stat("brushes 13/65535"), None);
        assert_eq!(parse_portal_stat("12 threads"), None);
    }
}
