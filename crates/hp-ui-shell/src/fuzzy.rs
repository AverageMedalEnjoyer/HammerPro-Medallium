pub fn fold(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    fold_into(s, &mut out);
    out
}

pub fn fold_into(s: &str, out: &mut String) {
    out.clear();
    for c in s.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
            continue;
        }
        for lower in c.to_lowercase() {
            match fold_char(lower) {
                Some(folded) => out.push_str(folded),
                None => out.push(lower),
            }
        }
    }
}

fn fold_char(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'č' => "c",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => "e",
        'ì' | 'í' | 'î' | 'ï' | 'ī' => "i",
        'ñ' | 'ń' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' => "o",
        'œ' => "oe",
        'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        'ś' | 'š' => "s",
        'ź' | 'ż' | 'ž' => "z",
        'ł' => "l",
        'ř' => "r",
        'ť' => "t",
        'ď' => "d",
        '\u{2019}' | '\u{2018}' => "'",
        _ => return None,
    })
}

fn is_separator(c: char) -> bool {
    matches!(c, ' ' | '_' | '-' | '/' | '\\' | '.' | '(' | ')' | '[' | ']' | ':' | ',' | '\'' | '+')
}

const SCORE_MATCH: i32 = 16;
const BONUS_FIRST_CHAR: i32 = 32;
const BONUS_WORD_START: i32 = 24;
const BONUS_CONSECUTIVE: i32 = 20;
const MAX_GAP_PENALTY: i32 = 12;
const MAX_CANDIDATE_CHARS: usize = 512;

fn token_score(token: &[char], cand: &[char]) -> Option<i32> {
    let n = cand.len();
    let m = token.len();
    if m == 0 {
        return Some(0);
    }
    if m > n {
        return None;
    }
    let position_bonus = |j: usize| -> i32 {
        if j == 0 {
            BONUS_FIRST_CHAR
        } else if is_separator(cand[j - 1]) && !is_separator(cand[j]) {
            BONUS_WORD_START
        } else {
            0
        }
    };
    const NONE: i32 = i32::MIN / 4;
    let mut prev = vec![NONE; n];
    let mut cur = vec![NONE; n];
    for j in 0..n {
        if cand[j] == token[0] {
            let lead = (j as i32).min(MAX_GAP_PENALTY) / 3;
            prev[j] = SCORE_MATCH + position_bonus(j) - lead;
        }
    }
    let max_gap = MAX_GAP_PENALTY as usize;
    for (i, &tc) in token.iter().enumerate().skip(1) {
        cur.iter_mut().for_each(|v| *v = NONE);
        let mut far_best = NONE;
        for j in i..n {
            if j > max_gap {
                let k = j - 1 - max_gap;
                if k >= i - 1 {
                    far_best = far_best.max(prev[k]);
                }
            }
            if cand[j] != tc {
                continue;
            }
            let mut best = NONE;
            if prev[j - 1] > NONE {
                best = prev[j - 1] + BONUS_CONSECUTIVE;
            }
            if far_best > NONE {
                best = best.max(far_best - MAX_GAP_PENALTY);
            }
            let lo = (i - 1).max(j.saturating_sub(max_gap));
            for (k, &p) in prev.iter().enumerate().take(j - 1).skip(lo) {
                if p > NONE {
                    best = best.max(p - (j - k - 1) as i32);
                }
            }
            if best > NONE {
                cur[j] = best + SCORE_MATCH + position_bonus(j);
            }
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev.into_iter().filter(|&v| v > NONE).max()
}

#[derive(Clone, Debug, Default)]
pub struct Query {
    tokens: Vec<Vec<char>>,
    folded: String,
}

impl Query {
    pub fn new(text: &str) -> Self {
        let folded = fold(text.trim()).replace('_', " ");
        let tokens = folded.split_whitespace().map(|t| t.chars().collect()).collect();
        Self { tokens, folded }
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn score_folded(&self, cand: &str) -> Option<i32> {
        if self.tokens.is_empty() {
            return Some(0);
        }
        let chars: Vec<char> = cand.chars().take(MAX_CANDIDATE_CHARS).collect();
        let mut total = 0;
        for token in &self.tokens {
            total += token_score(token, &chars)?;
        }
        if let Some(pos) = cand.find(self.folded.as_str()) {
            total += if pos == 0 { 40 } else { 20 };
        }
        total -= (chars.len() as i32) / 8;
        Some(total)
    }

    pub fn score(&self, cand: &str) -> Option<i32> {
        self.score_folded(&fold(cand))
    }

    pub fn best_of<'a>(&self, cands: impl IntoIterator<Item = &'a str>) -> Option<i32> {
        cands.into_iter().filter_map(|c| self.score_folded(c)).max()
    }
}

pub fn rank<'a, T>(query: &Query, items: &'a [T], haystacks: impl Fn(&'a T) -> Vec<&'a str>) -> Vec<(usize, i32)> {
    let mut out: Vec<(usize, i32)> =
        items.iter().enumerate().filter_map(|(i, item)| query.best_of(haystacks(item)).map(|s| (i, s))).collect();
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn order(query: &str, cands: &[&str]) -> Vec<String> {
        let q = Query::new(query);
        let folded: Vec<String> = cands.iter().map(|c| fold(c)).collect();
        rank(&q, &folded, |s| vec![s.as_str()]).into_iter().map(|(i, _)| cands[i].to_owned()).collect()
    }

    #[test]
    fn folding() {
        assert_eq!(fold("Aperçu de l'Éclairage"), "apercu de l'eclairage");
        assert_eq!(fold("ŒUVRE Straße"), "oeuvre strasse");
        assert_eq!(fold("Paramètres..."), "parametres...");
        assert_eq!(fold("l\u{2019}objet"), "l'objet");
    }

    #[test]
    fn subsequence_required() {
        let q = Query::new("xyz");
        assert_eq!(q.score("Save"), None);
        assert!(Query::new("sv").score("Save").is_some());
        assert!(Query::new("").score("anything").is_some());
        assert!(Query::new("   ").is_empty());
        assert!(Query::new("tool_clip").score("tool clip").is_some());
    }

    #[test]
    fn exact_and_prefix_win() {
        let r = order("save", &["Save as...", "Save", "Export selection as VMF...", "Snap selection to grid"]);
        assert_eq!(r[0], "Save");
        assert_eq!(r[1], "Save as...");
        assert!(!r.contains(&"Snap selection to grid".to_owned()));
    }

    #[test]
    fn word_starts_beat_scattered_letters() {
        let r = order("sa", &["Disable all", "Select all"]);
        assert_eq!(r[0], "Select all");
        let r = order("tl", &["Settle", "Texture lock"]);
        assert_eq!(r[0], "Texture lock");
    }

    #[test]
    fn tokens_in_any_order() {
        let r = order("tool sel", &["Entity tool", "Selection tool", "Select all"]);
        assert_eq!(r[0], "Selection tool");
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn accent_insensitive() {
        let q = Query::new("eclairage");
        assert!(q.score("Aperçu de l'éclairage").is_some());
        let q = Query::new("éclair");
        assert!(q.score("Lighting preview / eclairage").is_some());
    }

    #[test]
    fn consecutive_beats_gaps() {
        let q = Query::new("grid");
        let a = q.score("Show grid").unwrap_or(i32::MIN);
        let b = q.score("Go to brush or entity id").unwrap_or(i32::MIN);
        assert!(a > b, "{a} <= {b}");
    }

    #[test]
    fn ties_keep_original_order() {
        let r = order("", &["b", "a", "c"]);
        assert_eq!(r, ["b", "a", "c"]);
    }

    #[test]
    fn long_candidates_do_not_explode() {
        let long = "a".repeat(5000);
        assert!(Query::new("aaa").score(&long).is_some());
    }
}
