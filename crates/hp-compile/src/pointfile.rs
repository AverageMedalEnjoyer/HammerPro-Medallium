use std::path::{Path, PathBuf};

use crate::{CompileError, Result};

const MAX_POINTFILE_BYTES: u64 = 64 * 1024 * 1024;

pub fn read_pointfile(path: &Path) -> Result<Vec<[f64; 3]>> {
    let io_err = |source| CompileError::Io { path: path.to_path_buf(), source };
    let len = std::fs::metadata(path).map_err(io_err)?.len();
    if len > MAX_POINTFILE_BYTES {
        return Err(CompileError::Pointfile {
            path: path.to_path_buf(),
            line: 0,
            message: format!("file is too large ({len} bytes)"),
        });
    }
    let bytes = std::fs::read(path).map_err(io_err)?;
    let text = String::from_utf8_lossy(&bytes);
    parse_pointfile(&text).map_err(|e| match e {
        CompileError::Pointfile { line, message, .. } => {
            CompileError::Pointfile { path: path.to_path_buf(), line, message }
        }
        other => other,
    })
}

pub fn parse_pointfile(text: &str) -> Result<Vec<[f64; 3]>> {
    let mut points = Vec::with_capacity(text.len() / 24);
    for (index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let err = |message: String| CompileError::Pointfile { path: PathBuf::new(), line: index + 1, message };
        let mut point = [0.0f64; 3];
        let mut tokens = line.split_ascii_whitespace();
        for slot in &mut point {
            let tok = tokens.next().ok_or_else(|| err(format!("expected 3 numbers, got {line:?}")))?;
            let value: f64 = tok.parse().map_err(|_| err(format!("invalid number {tok:?}")))?;
            if !value.is_finite() {
                return Err(err(format!("non-finite number {tok:?}")));
            }
            *slot = value;
        }
        points.push(point);
    }
    Ok(points)
}

pub fn find_pointfile(vmf_path: &Path) -> Option<PathBuf> {
    ["lin", "pts"].into_iter().map(|ext| vmf_path.with_extension(ext)).find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_lin() {
        let text = "-512.000000 -512.000000 232.000000\r\n-680.000000 0.000000 8.000000\r\n";
        let pts = parse_pointfile(text).unwrap();
        assert_eq!(pts, vec![[-512.0, -512.0, 232.0], [-680.0, 0.0, 8.0]]);
    }

    #[test]
    fn blank_lines_and_extra_columns() {
        let pts = parse_pointfile("\n 1 2 3 extra\n\n4\t5\t6\n").unwrap();
        assert_eq!(pts, vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]]);
    }

    #[test]
    fn malformed_lines_are_errors() {
        assert!(matches!(parse_pointfile("1 2 3\n1 2\n"), Err(CompileError::Pointfile { line: 2, .. })));
        assert!(parse_pointfile("1 x 3").is_err());
        assert!(parse_pointfile("1 inf 3").is_err());
        assert!(parse_pointfile("NaN 1 3").is_err());
    }

    #[test]
    fn find_and_read_from_disk() {
        let dir = crate::test_util::scratch("pointfile");
        let vmf = dir.join("leaky.vmf");
        assert_eq!(find_pointfile(&vmf), None);
        std::fs::write(dir.join("leaky.pts"), "0 0 0\n").unwrap();
        assert_eq!(find_pointfile(&vmf), Some(dir.join("leaky.pts")));
        std::fs::write(dir.join("leaky.lin"), "1 2 3\n4 5 6\n").unwrap();
        let found = find_pointfile(&vmf).unwrap();
        assert_eq!(found, dir.join("leaky.lin"));
        assert_eq!(read_pointfile(&found).unwrap().len(), 2);
        assert!(matches!(read_pointfile(&dir.join("missing.lin")), Err(CompileError::Io { .. })));
        std::fs::write(dir.join("bad.lin"), "1 2 3\noops\n").unwrap();
        match read_pointfile(&dir.join("bad.lin")) {
            Err(CompileError::Pointfile { path, line, .. }) => {
                assert_eq!(line, 2);
                assert!(path.ends_with("bad.lin"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
