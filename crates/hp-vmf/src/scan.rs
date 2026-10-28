use std::borrow::Cow;

use hp_kv::{Block, ParseIssues};
use rayon::prelude::*;

use crate::{MAX_NESTING, VmfError};

pub(crate) struct Top<'a> {
    pub name: Cow<'a, str>,
    pub props: Vec<(String, String)>,
    pub children: Vec<Child<'a>>,
}

pub(crate) enum Child<'a> {
    Span { name: &'a str, text: &'a str },
    Parsed(Block),
}

impl Child<'_> {
    pub fn name(&self) -> &str {
        match self {
            Child::Span { name, .. } => name,
            Child::Parsed(b) => &b.name,
        }
    }

    pub fn into_block(self) -> Result<Block, VmfError> {
        match self {
            Child::Parsed(b) => Ok(b),
            Child::Span { name, text } => {
                let root = hp_kv::parse(text).map_err(|e| VmfError::Parse(e.to_string()))?;
                let mut block = root.children.into_iter().next().unwrap_or_default();
                if block.name != name {
                    block.name = name.to_owned();
                }
                Ok(block)
            }
        }
    }
}

impl Top<'_> {
    pub fn into_block(self) -> Result<Block, VmfError> {
        let children = self.children.into_iter().map(Child::into_block).collect::<Result<Vec<_>, _>>()?;
        Ok(Block { name: self.name.into_owned(), props: self.props, children })
    }
}

#[derive(Clone, Copy)]
enum Tok<'a> {
    Str(&'a str, usize),
    Open(usize),
    Close,
    Cond,
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    unterminated: bool,
    unclosed: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str) -> Self {
        let src = src.strip_prefix('\u{feff}').unwrap_or(src);
        Self { src, bytes: src.as_bytes(), pos: 0, unterminated: false, unclosed: 0 }
    }

    #[inline]
    fn byte(&self, i: usize) -> Option<u8> {
        self.bytes.get(i).copied()
    }

    #[inline]
    fn slice(&self, start: usize, end: usize) -> &'a str {
        self.src.get(start..end).unwrap_or("")
    }

    #[inline]
    fn skip_ws_and_comments(&mut self) {
        loop {
            while self.byte(self.pos).is_some_and(|c| c.is_ascii_whitespace()) {
                self.pos += 1;
            }
            if self.byte(self.pos) == Some(b'/') && self.byte(self.pos + 1) == Some(b'/') {
                let rest = self.bytes.get(self.pos..).unwrap_or_default();
                match memchr::memchr(b'\n', rest) {
                    Some(off) => self.pos += off + 1,
                    None => self.pos = self.bytes.len(),
                }
                continue;
            }
            break;
        }
    }

    #[inline]
    fn next(&mut self) -> Option<Tok<'a>> {
        self.skip_ws_and_comments();
        let len = self.bytes.len();
        let first = self.byte(self.pos)?;
        let tok_start = self.pos;
        match first {
            b'{' => {
                self.pos += 1;
                Some(Tok::Open(tok_start))
            }
            b'}' => {
                self.pos += 1;
                Some(Tok::Close)
            }
            b'"' => {
                let start = self.pos + 1;
                let rest = self.bytes.get(start..).unwrap_or_default();
                let end = match memchr::memchr(b'"', rest) {
                    Some(off) => start + off,
                    None => {
                        self.unterminated = true;
                        len
                    }
                };
                self.pos = (end + 1).min(len);
                Some(Tok::Str(self.slice(start, end), tok_start))
            }
            b'[' => {
                let rest = self.bytes.get(self.pos..).unwrap_or_default();
                self.pos = memchr::memchr(b']', rest).map_or(len, |off| self.pos + off + 1);
                Some(Tok::Cond)
            }
            _ => {
                while let Some(c) = self.byte(self.pos) {
                    if c.is_ascii_whitespace() || c == b'{' || c == b'}' || c == b'"' {
                        break;
                    }
                    if c == b'/' && self.byte(self.pos + 1) == Some(b'/') {
                        break;
                    }
                    self.pos += 1;
                }
                Some(Tok::Str(self.slice(tok_start, self.pos), tok_start))
            }
        }
    }

    fn skip_block(&mut self, base_depth: usize) -> Result<usize, VmfError> {
        let skipped = skip_from(self.bytes, self.pos, base_depth)?;
        Ok(self.apply(skipped))
    }

    fn apply(&mut self, skipped: Skipped) -> usize {
        self.pos = skipped.end;
        self.unterminated |= skipped.unterminated;
        self.unclosed += skipped.unclosed;
        skipped.end
    }

    #[cfg(test)]
    fn skip_block_by_tokens(&mut self, base_depth: usize) -> Result<usize, VmfError> {
        let mut depth = 1usize;
        while let Some(tok) = self.next() {
            match tok {
                Tok::Open(_) => {
                    depth += 1;
                    if base_depth + depth > MAX_NESTING {
                        return Err(too_deep());
                    }
                }
                Tok::Close => {
                    depth -= 1;
                    if depth == 0 {
                        return Ok(self.pos);
                    }
                }
                Tok::Str(..) | Tok::Cond => {}
            }
        }
        self.unclosed += depth;
        Ok(self.bytes.len())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Skipped {
    end: usize,
    unterminated: bool,
    unclosed: usize,
}

fn skip_from(b: &[u8], pos: usize, base_depth: usize) -> Result<Skipped, VmfError> {
    let len = b.len();
    let mut i = pos;
    let mut depth = 1usize;
    let mut token_start = true;
    let mut unterminated = false;
    let past = |needle: u8, from: usize| -> usize {
        let rest = b.get(from..).unwrap_or_default();
        if let Some(off) = rest.iter().take(24).position(|&c| c == needle) {
            return from + off + 1;
        }
        let skip = rest.len().min(24);
        rest.get(skip..).and_then(|r| memchr::memchr(needle, r)).map_or(len, |off| from + skip + off + 1)
    };
    while let Some(&c) = b.get(i) {
        match BYTE_CLASS[usize::from(c)] {
            WS => {
                i += 1;
                token_start = true;
            }
            QUOTE => {
                let from = i + 1;
                i = past(b'"', from);
                if i == len && (from >= len || b.get(len - 1) != Some(&b'"')) {
                    unterminated = true;
                }
                token_start = true;
            }
            OPEN => {
                depth += 1;
                if base_depth + depth > MAX_NESTING {
                    return Err(too_deep());
                }
                i += 1;
                token_start = true;
            }
            CLOSE => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Ok(Skipped { end: i, unterminated, unclosed: 0 });
                }
                token_start = true;
            }
            SLASH if b.get(i + 1) == Some(&b'/') => {
                i = past(b'\n', i);
                token_start = true;
            }
            BRACKET if token_start => {
                i = past(b']', i);
                token_start = true;
            }
            _ => {
                i += 1;
                token_start = false;
            }
        }
    }
    Ok(Skipped { end: len, unterminated, unclosed: depth })
}

const PRESKIP_MIN: usize = 4 << 20;

struct Preskip {
    entries: Vec<(usize, Option<Skipped>)>,
    next: usize,
}

impl Preskip {
    fn new(bytes: &[u8]) -> Self {
        if bytes.len() < PRESKIP_MIN {
            return Self { entries: Vec::new(), next: 0 };
        }
        const CHUNK: usize = 1 << 20;
        const NEEDLE: &[u8] = b"\n\t{";
        let finder = memchr::memmem::Finder::new(NEEDLE);
        let starts: Vec<usize> = (0..bytes.len()).step_by(CHUNK).collect();
        let mut braces: Vec<usize> = starts
            .par_iter()
            .flat_map_iter(|&start| {
                let end = (start + CHUNK + NEEDLE.len() - 1).min(bytes.len());
                let chunk = bytes.get(start..end).unwrap_or_default();
                finder.find_iter(chunk).map(move |at| start + at + NEEDLE.len()).collect::<Vec<_>>()
            })
            .collect();
        braces.sort_unstable();
        braces.dedup();
        let entries = braces.into_par_iter().map(|after| (after, skip_from(bytes, after, 1).ok())).collect();
        Self { entries, next: 0 }
    }

    fn take(&mut self, after: usize) -> Option<Option<Skipped>> {
        while self.entries.get(self.next).is_some_and(|(at, _)| *at < after) {
            self.next += 1;
        }
        match self.entries.get(self.next) {
            Some((at, skipped)) if *at == after => Some(*skipped),
            _ => None,
        }
    }
}

const OTHER: u8 = 0;
const WS: u8 = 1;
const QUOTE: u8 = 2;
const OPEN: u8 = 3;
const CLOSE: u8 = 4;
const SLASH: u8 = 5;
const BRACKET: u8 = 6;

static BYTE_CLASS: [u8; 256] = {
    let mut t = [OTHER; 256];
    t[b' ' as usize] = WS;
    t[b'\t' as usize] = WS;
    t[b'\n' as usize] = WS;
    t[b'\r' as usize] = WS;
    t[0x0C] = WS;
    t[b'"' as usize] = QUOTE;
    t[b'{' as usize] = OPEN;
    t[b'}' as usize] = CLOSE;
    t[b'/' as usize] = SLASH;
    t[b'[' as usize] = BRACKET;
    t
};

fn too_deep() -> VmfError {
    VmfError::Parse(format!(
        "the file is damaged or not a VMF document (blocks nested deeper than {MAX_NESTING} levels)"
    ))
}

const SNIFF_LEN: usize = 4096;

pub(crate) fn looks_binary(bytes: &[u8]) -> bool {
    let head = bytes.get(..SNIFF_LEN).unwrap_or(bytes);
    let mut controls = 0usize;
    for &b in head {
        match b {
            0 => return true,
            b'\t' | b'\n' | b'\r' | 0x0B | 0x0C | 0x1B => {}
            0x01..=0x1F | 0x7F => controls += 1,
            _ => {}
        }
    }
    controls * 32 > head.len()
}

pub(crate) fn scan(src: &str) -> Result<(Vec<Top<'_>>, ParseIssues), VmfError> {
    let mut lx = Lexer::new(src);
    let mut preskip = Preskip::new(lx.bytes);
    let mut issues = ParseIssues::default();
    let mut tops: Vec<Top<'_>> = Vec::new();
    let mut pending: Option<(&str, usize)> = None;
    let mut current: Option<usize> = None;

    while let Some(tok) = lx.next() {
        match tok {
            Tok::Cond => {}
            Tok::Str(s, start) => match pending.take() {
                None => pending = Some((s, start)),
                Some((key, _)) => {
                    if let Some(top) = current.and_then(|i| tops.get_mut(i)) {
                        top.props.push((key.to_owned(), s.to_owned()));
                    }
                }
            },
            Tok::Open(open_at) => match current {
                None => {
                    let name = pending.take().map_or("", |(n, _)| n);
                    tops.push(Top { name: Cow::Borrowed(name), props: Vec::new(), children: Vec::new() });
                    current = Some(tops.len() - 1);
                }
                Some(i) => {
                    let (name, start) = pending.take().unwrap_or(("", open_at));
                    let end = match preskip.take(lx.pos) {
                        Some(Some(skipped)) => lx.apply(skipped),
                        Some(None) => return Err(too_deep()),
                        None => lx.skip_block(1)?,
                    };
                    let text = lx.slice(start, end);
                    if let Some(top) = tops.get_mut(i) {
                        top.children.push(Child::Span { name, text });
                    }
                }
            },
            Tok::Close => {
                if pending.take().is_some() {
                    issues.missing_values += 1;
                }
                if current.take().is_none() {
                    issues.stray_closes += 1;
                }
            }
        }
    }
    if pending.is_some() {
        issues.missing_values += 1;
    }
    issues.unterminated_string = lx.unterminated;
    issues.unclosed_blocks = lx.unclosed + usize::from(current.is_some());
    Ok((tops, issues))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_to_root(src: &str) -> Block {
        let mut root = Block::new("");
        for top in scan(src).unwrap().0 {
            root.children.push(top.into_block().unwrap());
        }
        root
    }

    fn kv_root_without_props(src: &str) -> Block {
        let mut root = hp_kv::parse(src).unwrap();
        root.props.clear();
        root
    }

    #[test]
    fn matches_hp_kv_on_tricky_input() {
        let cases = [
            "world { \"id\" \"1\" solid { \"id\" \"2\" side { \"plane\" \"(0 0 0) (1 0 0) (1 1 0)\" } } }",
            "a{b{c{}}d{}}e{}",
            "// c { }\nworld // x }\n{ \"k\" \"v // not a comment\" solid [$WIN32] { \"q\" \"}{\" } }",
            "world { solid { side { ",
            "} } world { } } entity { \"k\" }",
            "\u{feff}versioninfo { \"a\" \"b\" }",
            "world { \"x\" solid { } \"y\" \"z\" { } }",
            "world { [$X360] { } \"a\" [$X] \"b\" }",
            "\"quoted name\" { \"inner block\" { \"k\" \"v\" } }",
            "w { \"unterminated { } ",
            "w { s { \"k\" \"v\"}{ } }",
            "w { s{\"a\"\"b\"}t{} }",
            "w { s { [unclosed { } } }",
            "w { s { } } // trailing",
            "loose \"pair\" w { x { } }",
            "w { \u{feff}solid { } }",
            "w { é { \"ü\" \"ñ\" } }",
        ];
        for src in cases {
            assert_eq!(scan_to_root(src), kv_root_without_props(src), "input: {src:?}");
            let (_, mut kv) = hp_kv::parse_with_issues(src);
            let (_, mut sc) = scan(src).unwrap();
            kv.missing_values = 0;
            sc.missing_values = 0;
            assert_eq!(sc, kv, "issues of input: {src:?}");
        }
    }

    #[test]
    fn reports_truncation() {
        let (_, issues) = scan("world { \"id\" \"1\" solid { side { \"plane\" \"(161 -1").unwrap();
        assert!(issues.unterminated_string);
        assert_eq!(issues.unclosed_blocks, 3);
        let (_, issues) = scan("world { solid { } }\nentity { \"id\" \"2\" ").unwrap();
        assert!(!issues.unterminated_string);
        assert_eq!(issues.unclosed_blocks, 1);
        let (_, issues) = scan("world { solid { side { } } }\n").unwrap();
        assert!(issues.is_clean());
        let (_, issues) = scan("} world { \"k\" }").unwrap();
        assert_eq!((issues.stray_closes, issues.missing_values), (1, 1));
    }

    #[test]
    fn fast_skip_matches_token_skip() {
        let alphabet = [
            "{",
            "}",
            "\"",
            "//",
            "/",
            "[",
            "]",
            "[$X]",
            " ",
            "\n",
            "\t",
            "a",
            "é",
            "\"q{\"",
            "\r\n",
            "x/y",
            "\x0b",
            "\x0c",
            "\"0123456789 0123456789 0123456789 0123456789\"",
            "unquoted_token_longer_than_24_bytes",
            "[$CONDITION_LONGER_THAN_24_BYTES]",
        ];
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut rand = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % 1_000_003) as usize
        };
        for _ in 0..50_000 {
            let mut s = String::new();
            for _ in 0..rand() % 48 {
                s.push_str(alphabet[rand() % alphabet.len()]);
            }
            for base in [1, MAX_NESTING - 3] {
                let mut fast = Lexer::new(&s);
                let mut slow = Lexer::new(&s);
                let a = fast.skip_block(base).map_err(|e| e.to_string());
                let b = slow.skip_block_by_tokens(base).map_err(|e| e.to_string());
                assert_eq!(a, b, "input {s:?}");
                if a.is_ok() {
                    assert_eq!(
                        (fast.unterminated, fast.unclosed),
                        (slow.unterminated, slow.unclosed),
                        "recoveries of input {s:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn preskipped_scans_match_sequential_ones() {
        let sequential = |src: &str| {
            let mut lx = Lexer::new(src);
            let mut out = Vec::new();
            let mut depth = 0usize;
            while let Some(tok) = lx.next() {
                match tok {
                    Tok::Open(_) if depth == 1 => {
                        let end = lx.skip_block(1).map_err(|e| e.to_string());
                        out.push(end);
                    }
                    Tok::Open(_) => depth += 1,
                    Tok::Close => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            (out, lx.unterminated, lx.unclosed)
        };
        let solid = "\tsolid\n\t{\n\t\t\"id\" \"2\"\n\t\tside\n\t\t{\n\t\t\t\"plane\" \"(0 0 0) (1 0 0) (1 1 0)\"\n\t\t}\n\t}\n";
        let tricky = [
            "\t\"message\" \"line\n\t{ not a block\"\n",
            "\t// comment\n\t{\n\t}\n",
            "\tsolid [$X]\n\t{\n\t}\n",
            "\tsolid\n\t{\n\t\t\"k\" \"}\"\n\t}\n",
        ];
        for (i, extra) in tricky.iter().enumerate() {
            let mut src = String::from("world\n{\n\t\"id\" \"1\"\n");
            while src.len() < PRESKIP_MIN + 1000 {
                src.push_str(solid);
                if src.len() % 7 == i {
                    src.push_str(extra);
                }
            }
            src.push_str(extra);
            src.push_str("}\nentity\n{\n\t\"id\" \"9\"\n");
            src.push_str(solid);
            src.push_str("}\n");
            let (tops, issues) = scan(&src).expect("scans");
            let reference = sequential(&src);
            let ends: Vec<Result<usize, String>> = tops
                .iter()
                .flat_map(|t| t.children.iter())
                .map(|c| match c {
                    Child::Span { text, .. } => Ok(text.as_ptr() as usize - src.as_ptr() as usize + text.len()),
                    Child::Parsed(_) => Err(String::new()),
                })
                .collect();
            assert_eq!(ends, reference.0, "case {i}");
            assert_eq!((issues.unterminated_string, issues.unclosed_blocks), (reference.1, reference.2), "case {i}");
        }
        let mut cut = String::from("world\n{\n");
        while cut.len() < PRESKIP_MIN + 100 {
            cut.push_str(solid);
        }
        cut.push_str("\tsolid\n\t{\n\t\t\"id\" \"3");
        let (_, issues) = scan(&cut).expect("scans");
        assert!(issues.unterminated_string);
        assert_eq!(issues.unclosed_blocks, 2);
    }

    #[test]
    fn spans_cover_blocks() {
        let src = "world\n{\n\t\"id\" \"1\"\n\tsolid\n\t{\n\t\t\"id\" \"2\"\n\t}\n}\n";
        let (tops, issues) = scan(src).unwrap();
        assert!(issues.is_clean());
        assert_eq!(tops.len(), 1);
        assert_eq!(tops[0].name, "world");
        assert_eq!(tops[0].props, vec![("id".to_owned(), "1".to_owned())]);
        match &tops[0].children[0] {
            Child::Span { name, text } => {
                assert_eq!(*name, "solid");
                assert_eq!(*text, "solid\n\t{\n\t\t\"id\" \"2\"\n\t}");
            }
            Child::Parsed(_) => panic!("expected a span"),
        }
    }

    #[test]
    fn binary_data_is_not_text() {
        assert!(looks_binary(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"));
        assert!(looks_binary(b"world { \"id\" \"1\0\" }"));
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let noise: Vec<u8> = (0..4096)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_le_bytes()[3].max(1)
            })
            .collect();
        assert!(looks_binary(&noise));
        assert!(!looks_binary(b""));
        assert!(!looks_binary("\u{feff}world\r\n{\r\n\t\"message\" \"caf\u{e9}\"\x0c\r\n}".as_bytes()));
        let outputs = "\"OnTrigger\" \"door\x1bOpen\x1b\x1b0\x1b-1\"\n".repeat(100);
        assert!(!looks_binary(outputs.as_bytes()));
        assert!(!looks_binary(b"world { \"message\" \"caf\xe9\" }"));
        let mut text = "world { \"id\" \"1\" }\n".repeat(200).into_bytes();
        text[100] = 0x07;
        assert!(!looks_binary(&text));
    }

    #[test]
    fn rejects_deep_nesting() {
        let deep = "a{".repeat(MAX_NESTING + 5);
        assert!(matches!(scan(&deep), Err(VmfError::Parse(_))));
        let ok = "a{".repeat(MAX_NESTING);
        assert!(scan(&ok).is_ok());
    }
}
