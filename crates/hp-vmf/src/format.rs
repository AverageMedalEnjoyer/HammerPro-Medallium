pub use hp_kv::TextEncoding;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    pub fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum NumberStyle {
    #[default]
    Plain,
    Exponent(u8),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextFormat {
    pub encoding: TextEncoding,
    pub line_ending: LineEnding,
    pub bom: bool,
    pub numbers: NumberStyle,
}

impl TextFormat {
    pub fn detect(bytes: &[u8]) -> Self {
        let bom = bytes.starts_with(b"\xEF\xBB\xBF");
        let line_ending = match memchr::memchr(b'\n', bytes) {
            Some(i) if i > 0 && bytes.get(i - 1) == Some(&b'\r') => LineEnding::CrLf,
            _ => LineEnding::Lf,
        };
        Self { encoding: TextEncoding::Utf8, line_ending, bom, numbers: detect_numbers(bytes) }
    }
}

fn detect_numbers(bytes: &[u8]) -> NumberStyle {
    let mut best: Option<(usize, u8)> = None;
    for needle in [&b"e-0"[..], &b"e+0"[..]] {
        for at in memchr::memmem::find_iter(bytes, needle) {
            if best.is_some_and(|(b, _)| b < at) {
                break;
            }
            if at == 0 || !bytes.get(at - 1).is_some_and(u8::is_ascii_digit) {
                continue;
            }
            let digits = bytes.get(at + 2..).map_or(0, |rest| rest.iter().take_while(|b| b.is_ascii_digit()).count());
            let after = bytes.get(at + 2 + digits).copied();
            let ends = after.is_none_or(|c| c.is_ascii_whitespace() || matches!(c, b'"' | b')' | b']'));
            if (2..=4).contains(&digits) && ends {
                best = Some((at, u8::try_from(digits).unwrap_or(3)));
                break;
            }
        }
    }
    best.map_or(NumberStyle::Plain, |(_, d)| NumberStyle::Exponent(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection() {
        let f = TextFormat::detect(b"versioninfo\r\n{\r\n\t\"row0\" \"0 0 1 -1.19209e-007 0\"\r\n}\r\n");
        assert_eq!(f.line_ending, LineEnding::CrLf);
        assert_eq!(f.numbers, NumberStyle::Exponent(3));
        assert!(!f.bom);
        let f = TextFormat::detect(b"\xEF\xBB\xBFa\n{\n\"x\" \"2e+06\"\n}\n");
        assert_eq!((f.line_ending, f.numbers, f.bom), (LineEnding::Lf, NumberStyle::Exponent(2), true));
        let f = TextFormat::detect(b"\"material\" \"tile-01\" \"x\" \"blue-07a\" \"y\" \"3e-0x\"");
        assert_eq!(f.numbers, NumberStyle::Plain);
        assert_eq!(TextFormat::detect(b"").line_ending, LineEnding::Lf);
    }
}
