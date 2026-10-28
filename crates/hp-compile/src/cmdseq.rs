#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Special {
    None,
    ChangeDir,
    Copy,
    Delete,
    Rename,
}

impl Special {
    fn from_code(code: i32) -> Special {
        match code {
            256 => Special::ChangeDir,
            257 => Special::Copy,
            258 => Special::Delete,
            259 => Special::Rename,
            _ => Special::None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeqCommand {
    pub enabled: bool,
    pub special: Special,
    pub run: String,
    pub params: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sequence {
    pub name: String,
    pub commands: Vec<SeqCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CmdSeqError {
    #[error("not a Hammer command sequence file")]
    NotCmdSeq,
    #[error("the command sequence file is damaged")]
    Damaged,
    #[error("unknown command sequence version {0}")]
    Version(String),
}

const SIGNATURE: &[u8] = b"Worldcraft Command Sequences\r\n\x1a";
const MAX_PATH: usize = 260;
const MAX_SEQUENCES: usize = 256;
const MAX_COMMANDS: usize = 256;
pub const MAX_FILE_BYTES: usize = 16 * 1024 * 1024;

fn c_string(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
    crate::decode::decode_bytes(bytes.get(..end).unwrap_or_default()).into_owned()
}

struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], CmdSeqError> {
        let end = self.pos.checked_add(n).ok_or(CmdSeqError::Damaged)?;
        let s = self.bytes.get(self.pos..end).ok_or(CmdSeqError::Damaged)?;
        self.pos = end;
        Ok(s)
    }

    fn i32(&mut self) -> Result<i32, CmdSeqError> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32, CmdSeqError> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

pub fn parse_cmdseq_wc(bytes: &[u8]) -> Result<Vec<Sequence>, CmdSeqError> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(CmdSeqError::Damaged);
    }
    let rest = bytes.strip_prefix(SIGNATURE).ok_or(CmdSeqError::NotCmdSeq)?;
    let mut r = Reader { bytes: rest, pos: 0 };
    let version = r.f32()?;
    let v2 = if (version - 0.2).abs() < 1e-4 {
        true
    } else if (version - 0.1).abs() < 1e-4 {
        false
    } else {
        return Err(CmdSeqError::Version(format!("{version}")));
    };
    let count = usize::try_from(r.i32()?).map_err(|_| CmdSeqError::Damaged)?;
    if count > MAX_SEQUENCES {
        return Err(CmdSeqError::Damaged);
    }
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let name = c_string(r.take(128)?);
        let n = usize::try_from(r.i32()?).map_err(|_| CmdSeqError::Damaged)?;
        if n > MAX_COMMANDS {
            return Err(CmdSeqError::Damaged);
        }
        let mut commands = Vec::with_capacity(n);
        for _ in 0..n {
            let enabled = r.i32()? != 0;
            let special = Special::from_code(r.i32()?);
            let run = c_string(r.take(MAX_PATH)?);
            let params = c_string(r.take(MAX_PATH)?);
            let _long_filenames = r.i32()?;
            let _ensure_check = r.i32()?;
            let _ensure_file = r.take(MAX_PATH)?;
            if v2 {
                let _use_process_window = r.i32()?;
                let _no_wait = r.i32()?;
            }
            commands.push(SeqCommand { enabled, special, run, params });
        }
        out.push(Sequence { name, commands });
    }
    Ok(out)
}

pub fn parse_hammerpp_cfg(text: &str) -> Result<Vec<Sequence>, CmdSeqError> {
    if text.len() > MAX_FILE_BYTES {
        return Err(CmdSeqError::Damaged);
    }
    let root = hp_kv::parse(text).map_err(|_| CmdSeqError::Damaged)?;
    let top = root
        .children
        .iter()
        .find(|b| b.name.eq_ignore_ascii_case("Command Sequences"))
        .ok_or(CmdSeqError::NotCmdSeq)?;
    let mut out = Vec::new();
    for seq in top.children.iter().take(MAX_SEQUENCES) {
        let mut numbered: Vec<(u32, &hp_kv::Block)> =
            seq.children.iter().filter_map(|c| c.name.trim().parse::<u32>().ok().map(|n| (n, c))).collect();
        numbered.sort_by_key(|(n, _)| *n);
        let commands = numbered
            .into_iter()
            .take(MAX_COMMANDS)
            .map(|(_, c)| SeqCommand {
                enabled: c.get("enable").is_none_or(|v| v.trim() != "0"),
                special: Special::from_code(c.get("specialcmd").and_then(|v| v.trim().parse().ok()).unwrap_or(0)),
                run: c.get("run").unwrap_or_default().to_owned(),
                params: c.get("parms").or_else(|| c.get("params")).unwrap_or_default().to_owned(),
            })
            .collect();
        out.push(Sequence { name: seq.name.clone(), commands });
    }
    Ok(out)
}

pub fn parse_any(bytes: &[u8]) -> Result<Vec<Sequence>, CmdSeqError> {
    if bytes.starts_with(SIGNATURE) {
        return parse_cmdseq_wc(bytes);
    }
    parse_hammerpp_cfg(&crate::decode::decode_bytes(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed(s: &str, n: usize) -> Vec<u8> {
        let mut v = s.as_bytes().to_vec();
        v.resize(n, 0);
        v
    }

    fn wc(version: f32, cmds: &[(i32, i32, &str, &str)]) -> Vec<u8> {
        let mut v = SIGNATURE.to_vec();
        v.extend_from_slice(&version.to_le_bytes());
        v.extend_from_slice(&1i32.to_le_bytes());
        v.extend(fixed("Default", 128));
        v.extend_from_slice(&(cmds.len() as i32).to_le_bytes());
        for (en, sp, run, parms) in cmds {
            v.extend_from_slice(&en.to_le_bytes());
            v.extend_from_slice(&sp.to_le_bytes());
            v.extend(fixed(run, MAX_PATH));
            v.extend(fixed(parms, MAX_PATH));
            v.extend_from_slice(&0i32.to_le_bytes());
            v.extend_from_slice(&0i32.to_le_bytes());
            v.extend(fixed("", MAX_PATH));
            if version > 0.15 {
                v.extend_from_slice(&0i32.to_le_bytes());
                v.extend_from_slice(&0i32.to_le_bytes());
            }
        }
        v
    }

    #[test]
    fn binary_files() {
        for version in [0.1f32, 0.2] {
            let data = wc(
                version,
                &[
                    (1, 0, "$bsp_exe", "-game $gamedir $path\\$file"),
                    (0, 257, "", "$path\\$file.bsp $bspdir\\$file.bsp"),
                ],
            );
            let seqs = parse_cmdseq_wc(&data).expect("parse");
            assert_eq!(seqs.len(), 1);
            assert_eq!(seqs[0].name, "Default");
            assert_eq!(seqs[0].commands[0].run, "$bsp_exe");
            assert!(seqs[0].commands[0].enabled && !seqs[0].commands[1].enabled);
            assert_eq!(seqs[0].commands[1].special, Special::Copy);
            assert_eq!(parse_any(&data).expect("any"), seqs);
            for cut in 0..data.len() {
                let _ = parse_cmdseq_wc(&data[..cut]);
            }
        }
        assert_eq!(parse_cmdseq_wc(b"nope"), Err(CmdSeqError::NotCmdSeq));
        let mut bad = SIGNATURE.to_vec();
        bad.extend_from_slice(&3.0f32.to_le_bytes());
        assert!(matches!(parse_cmdseq_wc(&bad), Err(CmdSeqError::Version(_))));
        let mut huge = SIGNATURE.to_vec();
        huge.extend_from_slice(&0.2f32.to_le_bytes());
        huge.extend_from_slice(&i32::MAX.to_le_bytes());
        assert_eq!(parse_cmdseq_wc(&huge), Err(CmdSeqError::Damaged));
    }

    #[test]
    fn hammerpp_text() {
        let text = "\"Command Sequences\"\n{\n\t\"Fast\"\n\t{\n\t\t\"2\"\n\t\t{\n\t\t\t\"enable\" \"1\"\n\t\t\t\"specialcmd\" \"0\"\n\t\t\t\"run\" \"$vis_exe\"\n\t\t\t\"parms\" \"-fast -game $gamedir $path\\$file\"\n\t\t}\n\t\t\"1\"\n\t\t{\n\t\t\t\"enable\" \"0\"\n\t\t\t\"specialcmd\" \"258\"\n\t\t\t\"parms\" \"$path\\$file.prt\"\n\t\t}\n\t}\n}\n";
        let seqs = parse_hammerpp_cfg(text).expect("parse");
        assert_eq!(seqs[0].name, "Fast");
        assert_eq!(seqs[0].commands.len(), 2);
        assert_eq!(seqs[0].commands[0].special, Special::Delete, "numbered order");
        assert!(!seqs[0].commands[0].enabled);
        assert_eq!(seqs[0].commands[1].params, "-fast -game $gamedir $path\\$file");
        assert_eq!(parse_hammerpp_cfg("\"Other\" {}"), Err(CmdSeqError::NotCmdSeq));
    }

    #[test]
    fn real_files() {
        let bin = std::path::Path::new(r"D:\SteamLibrary\steamapps\common\GarrysMod\bin");
        if let Ok(bytes) = std::fs::read(bin.join("CmdSeqDefault.wc")) {
            let seqs = parse_any(&bytes).expect("CmdSeqDefault.wc");
            assert!(!seqs.is_empty());
            assert!(seqs.iter().any(|s| s.commands.iter().any(|c| c.run.eq_ignore_ascii_case("$bsp_exe"))));
        }
        if let Ok(bytes) = std::fs::read(bin.join("win64/hammerplusplus/hammerplusplus_sequences.cfg")) {
            let seqs = parse_any(&bytes).expect("hammerplusplus_sequences.cfg");
            let default = seqs.iter().find(|s| s.name == "Default").expect("Default");
            assert_eq!(default.commands.first().map(|c| c.run.as_str()), Some("$bsp_exe"));
            assert!(default.commands.iter().any(|c| c.special == Special::Copy && c.params.contains("$bspdir")));
        }
    }
}
