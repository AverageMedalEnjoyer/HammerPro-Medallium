#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tok<'a> {
    Str { raw: &'a str, escaped: bool },
    Word(&'a str),
    Directive(&'a str),
    Punct(u8),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Token<'a> {
    pub tok: Tok<'a>,
    pub line: u32,
}

pub(crate) struct Lexed<'a> {
    pub tokens: Vec<Token<'a>>,
    pub diags: Vec<(u32, &'static str)>,
}

#[inline]
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | 0x0B | 0x0C)
}

#[inline]
fn ends_word(b: u8) -> bool {
    is_space(b) || matches!(b, b'(' | b')' | b'[' | b']' | b':' | b'=' | b',' | b'"')
}

fn scan_word(bytes: &[u8], start: usize) -> usize {
    let mut j = start;
    while let Some(&b) = bytes.get(j) {
        if ends_word(b) || (b == b'/' && bytes.get(j + 1) == Some(&b'/')) {
            break;
        }
        j += 1;
    }
    j
}

pub(crate) fn lex(src: &str) -> Lexed<'_> {
    let bytes = src.as_bytes();
    let mut tokens = Vec::with_capacity(bytes.len() / 6 + 16);
    let mut diags = Vec::new();
    let mut line: u32 = 1;
    let mut i = 0usize;
    let slice = |a: usize, b: usize| src.get(a..b).unwrap_or("");

    while let Some(&c) = bytes.get(i) {
        match c {
            b'\n' => {
                line = line.saturating_add(1);
                i += 1;
            }
            b'\r' if bytes.get(i + 1) != Some(&b'\n') => {
                line = line.saturating_add(1);
                i += 1;
            }
            _ if is_space(c) => i += 1,
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = match bytes.get(i..).and_then(|rest| rest.iter().position(|&b| b == b'\n' || b == b'\r')) {
                    Some(p) => i + p,
                    None => bytes.len(),
                };
            }
            b'"' => {
                let start = i + 1;
                let mut j = start;
                let mut escaped = false;
                let end;
                loop {
                    match bytes.get(j) {
                        None => {
                            diags.push((line, "unterminated string at end of file"));
                            end = j;
                            i = j;
                            break;
                        }
                        Some(b'"') => {
                            end = j;
                            i = j + 1;
                            break;
                        }
                        Some(b'\n' | b'\r') => {
                            diags.push((line, "unterminated string (closed at end of line)"));
                            end = j;
                            i = j;
                            break;
                        }
                        Some(b'\\') => {
                            escaped = true;
                            j += match bytes.get(j + 1) {
                                Some(b'\n' | b'\r') | None => 1,
                                Some(_) => 2,
                            };
                        }
                        Some(_) => j += 1,
                    }
                }
                tokens.push(Token { tok: Tok::Str { raw: slice(start, end), escaped }, line });
            }
            b'(' | b')' | b'[' | b']' | b':' | b'=' | b',' | b'+' => {
                tokens.push(Token { tok: Tok::Punct(c), line });
                i += 1;
            }
            b'@' => {
                let end = scan_word(bytes, i + 1);
                if end == i + 1 {
                    diags.push((line, "stray '@' ignored"));
                    i += 1;
                } else {
                    tokens.push(Token { tok: Tok::Directive(slice(i + 1, end)), line });
                    i = end;
                }
            }
            _ => {
                let end = scan_word(bytes, i).max(i + 1);
                tokens.push(Token { tok: Tok::Word(slice(i, end)), line });
                i = end;
            }
        }
    }
    Lexed { tokens, diags }
}

pub(crate) fn unescape_into(raw: &str, out: &mut String) {
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let replacement = match chars.clone().next() {
            Some('n') => Some('\n'),
            Some('t') => Some('\t'),
            Some('"') => Some('"'),
            Some('\\') => Some('\\'),
            _ => None,
        };
        match replacement {
            Some(r) => {
                out.push(r);
                chars.next();
            }
            None => out.push('\\'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(src: &str) -> Vec<Tok<'_>> {
        lex(src).tokens.into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn basic_tokens() {
        let t = toks("@PointClass base(A, B) = foo : \"Desc\" [ ]");
        assert_eq!(
            t,
            vec![
                Tok::Directive("PointClass"),
                Tok::Word("base"),
                Tok::Punct(b'('),
                Tok::Word("A"),
                Tok::Punct(b','),
                Tok::Word("B"),
                Tok::Punct(b')'),
                Tok::Punct(b'='),
                Tok::Word("foo"),
                Tok::Punct(b':'),
                Tok::Str { raw: "Desc", escaped: false },
                Tok::Punct(b'['),
                Tok::Punct(b']'),
            ]
        );
    }

    #[test]
    fn comments_and_lines() {
        let lexed = lex("a // comment \"x\"\n// full line\nb//tail\n\"s // not a comment\"");
        let t: Vec<_> = lexed.tokens.iter().map(|t| (t.tok, t.line)).collect();
        assert_eq!(
            t,
            vec![(Tok::Word("a"), 1), (Tok::Word("b"), 3), (Tok::Str { raw: "s // not a comment", escaped: false }, 4),]
        );
        assert!(lexed.diags.is_empty());
    }

    #[test]
    fn negative_numbers_and_star_types() {
        assert_eq!(
            toks("size(-16 -16 0)(*studio)"),
            vec![
                Tok::Word("size"),
                Tok::Punct(b'('),
                Tok::Word("-16"),
                Tok::Word("-16"),
                Tok::Word("0"),
                Tok::Punct(b')'),
                Tok::Punct(b'('),
                Tok::Word("*studio"),
                Tok::Punct(b')'),
            ]
        );
    }

    #[test]
    fn plus_concatenation_and_inner_plus() {
        assert_eq!(
            toks("\"a\"+\"b\" 1e+5"),
            vec![
                Tok::Str { raw: "a", escaped: false },
                Tok::Punct(b'+'),
                Tok::Str { raw: "b", escaped: false },
                Tok::Word("1e+5"),
            ]
        );
    }

    #[test]
    fn escapes() {
        let t = toks(r#""line\nnext \"q\" C:\temp\\x""#);
        let Some(Tok::Str { raw, escaped: true }) = t.first().copied() else {
            panic!("expected escaped string, got {t:?}");
        };
        let mut out = String::new();
        unescape_into(raw, &mut out);
        assert_eq!(out, "line\nnext \"q\" C:\temp\\x");

        let mut out = String::new();
        unescape_into(r"hl2\scripts\", &mut out);
        assert_eq!(out, r"hl2\scripts\");
    }

    #[test]
    fn unterminated_strings_recover() {
        let lexed = lex("\"abc\r\nnext \"tail");
        let t: Vec<_> = lexed.tokens.iter().map(|t| t.tok).collect();
        assert_eq!(
            t,
            vec![Tok::Str { raw: "abc", escaped: false }, Tok::Word("next"), Tok::Str { raw: "tail", escaped: false },]
        );
        assert_eq!(lexed.diags.len(), 2);
    }

    #[test]
    fn stray_at_and_backslash_at_eof() {
        let lexed = lex("@ @Foo \"x\\");
        let t: Vec<_> = lexed.tokens.iter().map(|t| t.tok).collect();
        assert_eq!(t, vec![Tok::Directive("Foo"), Tok::Str { raw: "x\\", escaped: true }]);
        assert_eq!(lexed.diags.len(), 2);
    }

    #[test]
    fn carriage_return_only_line_endings() {
        let lexed = lex("a // c\rb\r\"s\rc");
        let t: Vec<_> = lexed.tokens.iter().map(|t| (t.tok, t.line)).collect();
        assert_eq!(
            t,
            vec![
                (Tok::Word("a"), 1),
                (Tok::Word("b"), 2),
                (Tok::Str { raw: "s", escaped: false }, 3),
                (Tok::Word("c"), 4)
            ]
        );
        assert_eq!(lexed.diags.len(), 1);
    }

    #[test]
    fn non_ascii_words() {
        assert_eq!(toks("caf\u{e9}:x"), vec![Tok::Word("caf\u{e9}"), Tok::Punct(b':'), Tok::Word("x")]);
    }
}
