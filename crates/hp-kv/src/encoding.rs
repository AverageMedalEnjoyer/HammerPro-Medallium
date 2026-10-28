use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum TextEncoding {
    #[default]
    Utf8,
    Windows1252,
}

const HIGH: [char; 32] = [
    '\u{20ac}', '\u{81}', '\u{201a}', '\u{192}', '\u{201e}', '\u{2026}', '\u{2020}', '\u{2021}', '\u{2c6}', '\u{2030}',
    '\u{160}', '\u{2039}', '\u{152}', '\u{8d}', '\u{17d}', '\u{8f}', '\u{90}', '\u{2018}', '\u{2019}', '\u{201c}',
    '\u{201d}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2dc}', '\u{2122}', '\u{161}', '\u{203a}', '\u{153}', '\u{9d}',
    '\u{17e}', '\u{178}',
];

pub fn decode_windows_1252(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() + bytes.len() / 8);
    for &b in bytes {
        let c = match b {
            0x80..=0x9F => HIGH.get(usize::from(b - 0x80)).copied().unwrap_or(char::from(b)),
            _ => char::from(b),
        };
        out.push(c);
    }
    out
}

#[inline]
fn byte_1252(c: char) -> Option<u8> {
    let code = u32::from(c);
    if code < 0x80 || (0xA0..=0xFF).contains(&code) {
        return u8::try_from(code).ok();
    }
    let i = HIGH.iter().position(|&h| h == c)?;
    u8::try_from(0x80 + i).ok()
}

pub fn encode_windows_1252(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len());
    for c in text.chars() {
        out.push(byte_1252(c)?);
    }
    Some(out)
}

pub fn decode(bytes: &[u8]) -> (Cow<'_, str>, TextEncoding) {
    match std::str::from_utf8(bytes) {
        Ok(s) => (Cow::Borrowed(s), TextEncoding::Utf8),
        Err(_) => (Cow::Owned(decode_windows_1252(bytes)), TextEncoding::Windows1252),
    }
}

pub fn encode(text: &str, encoding: TextEncoding) -> (Cow<'_, [u8]>, TextEncoding) {
    match encoding {
        TextEncoding::Utf8 => (Cow::Borrowed(text.as_bytes()), TextEncoding::Utf8),
        TextEncoding::Windows1252 => {
            if text.is_ascii() {
                return (Cow::Borrowed(text.as_bytes()), TextEncoding::Windows1252);
            }
            match encode_windows_1252(text) {
                Some(bytes) => (Cow::Owned(bytes), TextEncoding::Windows1252),
                None => (Cow::Borrowed(text.as_bytes()), TextEncoding::Utf8),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_byte_round_trips() {
        let all: Vec<u8> = (0..=255u8).collect();
        let text = decode_windows_1252(&all);
        assert_eq!(text.chars().count(), 256);
        assert_eq!(encode_windows_1252(&text).as_deref(), Some(&all[..]));
        let (back, enc) = encode(&text, TextEncoding::Windows1252);
        assert_eq!((&back[..], enc), (&all[..], TextEncoding::Windows1252));
    }

    #[test]
    fn decode_picks_the_encoding() {
        let (t, e) = decode("porte_\u{e9}".as_bytes());
        assert_eq!((t.as_ref(), e), ("porte_\u{e9}", TextEncoding::Utf8));
        assert!(matches!(t, Cow::Borrowed(_)));
        let (t, e) = decode(b"porte_\xe9 \x80 \x93");
        assert_eq!((t.as_ref(), e), ("porte_\u{e9} \u{20ac} \u{201c}", TextEncoding::Windows1252));
    }

    #[test]
    fn unrepresentable_text_falls_back_to_utf8() {
        let (bytes, enc) = encode("caf\u{e9} \u{65e5}", TextEncoding::Windows1252);
        assert_eq!(enc, TextEncoding::Utf8);
        assert_eq!(&bytes[..], "caf\u{e9} \u{65e5}".as_bytes());
        assert_eq!(encode_windows_1252("\u{65e5}"), None);
        let (bytes, enc) = encode("abc", TextEncoding::Windows1252);
        assert_eq!((&bytes[..], enc), (&b"abc"[..], TextEncoding::Windows1252));
    }
}
