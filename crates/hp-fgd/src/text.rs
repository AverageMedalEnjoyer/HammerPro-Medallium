use std::borrow::Cow;

const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{0081}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}',
    '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{008D}', '\u{017D}', '\u{008F}', '\u{0090}', '\u{2018}',
    '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}',
    '\u{203A}', '\u{0153}', '\u{009D}', '\u{017E}', '\u{0178}',
];

pub fn decode_text(bytes: &[u8]) -> Cow<'_, str> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return decode_8bit(rest);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return Cow::Owned(decode_utf16(rest, u16::from_le_bytes));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return Cow::Owned(decode_utf16(rest, u16::from_be_bytes));
    }
    decode_8bit(bytes)
}

fn decode_8bit(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => Cow::Owned(decode_cp1252(bytes)),
    }
}

fn decode_cp1252(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 8);
    for &b in bytes {
        let c = match b {
            0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
            _ => char::from(b),
        };
        out.push(c);
    }
    out
}

fn decode_utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|&pair| unit(pair));
    char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_is_borrowed() {
        let text = decode_text("@PointClass = a []".as_bytes());
        assert!(matches!(text, Cow::Borrowed(_)));
    }

    #[test]
    fn utf8_bom_is_stripped() {
        assert_eq!(decode_text(b"\xEF\xBB\xBF// hi"), "// hi");
    }

    #[test]
    fn cp1252_fallback() {
        assert_eq!(decode_text(b"// \xA9 Valve \x80"), "// \u{A9} Valve \u{20AC}");
    }

    #[test]
    fn utf16_le_and_be() {
        assert_eq!(decode_text(b"\xFF\xFEa\x00b\x00"), "ab");
        assert_eq!(decode_text(b"\xFE\xFF\x00a\x00b"), "ab");
        assert_eq!(decode_text(b"\xFF\xFE\x00\xD8x"), "\u{FFFD}");
    }
}
