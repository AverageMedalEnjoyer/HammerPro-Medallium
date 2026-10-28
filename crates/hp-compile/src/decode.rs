use std::borrow::Cow;

const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{02C6}',
    '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}', '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}',
    '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{FFFD}', '\u{FFFD}', '\u{02DC}', '\u{2122}', '\u{0161}',
    '\u{203A}', '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
];

pub(crate) fn decode_bytes(bytes: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(bytes) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => Cow::Owned(bytes.iter().map(|&b| cp1252_char(b)).collect()),
    }
}

fn cp1252_char(b: u8) -> char {
    match b {
        0x80..=0x9F => CP1252_HIGH.get(usize::from(b - 0x80)).copied().unwrap_or('\u{FFFD}'),
        _ => char::from(b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_is_borrowed() {
        assert!(matches!(decode_bytes("héllo".as_bytes()), Cow::Borrowed("héllo")));
    }

    #[test]
    fn cp1252_fallback() {
        let bytes = b"C:\\Users\\Cl\xE9ment \x80";
        assert_eq!(decode_bytes(bytes), "C:\\Users\\Clément \u{20AC}");
    }
}
