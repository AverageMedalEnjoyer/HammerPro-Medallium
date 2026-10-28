use std::borrow::Cow;

pub(crate) fn material_key(name: &str) -> String {
    hp_vmt::normalize_material_name(name)
}

pub(crate) fn is_normalized_material(name: &str) -> bool {
    !name.is_empty()
        && !name.bytes().any(|b| b.is_ascii_uppercase() || b == b'\\' || b == b' ')
        && !name.starts_with('/')
        && !name.starts_with("materials/")
        && !name.ends_with(".vmt")
        && !name.ends_with(".vtf")
}

pub(crate) fn model_key(path: &str) -> String {
    hp_fs::normalize_path(path.trim()).into_owned()
}

pub(crate) fn model_key_cow(path: &str) -> Cow<'_, str> {
    let p = path.trim();
    if !p.bytes().any(|b| b.is_ascii_uppercase() || b == b'\\') && !p.starts_with('/') && !p.contains("//") {
        Cow::Borrowed(p)
    } else {
        Cow::Owned(model_key(p))
    }
}

pub(crate) fn is_model_path(path: &str) -> bool {
    let p = path.trim();
    p.len() > 4 && p.get(p.len() - 4..).is_some_and(|ext| ext.eq_ignore_ascii_case(".mdl"))
}

pub(crate) fn sprite_material(path: &str) -> Option<String> {
    let p = path.trim();
    if p.is_empty() || is_model_path(p) {
        return None;
    }
    let mut key = material_key(p);
    if let Some(stripped) = key.strip_suffix(".spr") {
        key = stripped.to_owned();
    }
    (!key.is_empty()).then_some(key)
}

pub(crate) fn sanitize_stem(stem: &str) -> Option<String> {
    let mut out = String::with_capacity(stem.len());
    for c in stem.chars() {
        let lower = c.to_lowercase().next().unwrap_or(c);
        if lower.is_ascii_alphanumeric() || matches!(lower, '_' | '-') {
            out.push(lower);
        } else if let Some(folded) = fold_accent(lower) {
            out.push_str(folded);
        } else {
            out.push('_');
        }
    }
    let trimmed = out.trim_matches(|c| c == '_' || c == '-');
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

pub(crate) fn normalize_prefix(prefix: &str) -> String {
    let p = material_key(prefix);
    p.trim_matches('/').to_owned()
}

fn fold_accent(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' => "a",
        'æ' => "ae",
        'ç' => "c",
        'è' | 'é' | 'ê' | 'ë' => "e",
        'ì' | 'í' | 'î' | 'ï' => "i",
        'ñ' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' => "o",
        'œ' => "oe",
        'ù' | 'ú' | 'û' | 'ü' => "u",
        'ý' | 'ÿ' => "y",
        'ß' => "ss",
        _ => return None,
    })
}

pub(crate) fn parse_floats<const N: usize>(s: &str) -> ([f64; N], usize) {
    let mut out = [0.0; N];
    let mut n = 0;
    for (slot, tok) in out.iter_mut().zip(s.split_whitespace()) {
        match tok.parse::<f64>() {
            Ok(v) if v.is_finite() => {
                *slot = v;
                n += 1;
            }
            _ => break,
        }
    }
    (out, n)
}

pub(crate) struct Budget {
    start: std::time::Instant,
    limit: std::time::Duration,
}

impl Budget {
    pub(crate) fn new(limit: std::time::Duration) -> Self {
        Self { start: std::time::Instant::now(), limit }
    }

    pub(crate) fn exceeded(&self) -> bool {
        self.start.elapsed() >= self.limit
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn material_names() {
        assert_eq!(material_key("Materials\\Concrete\\ConcreteFloor001a.vmt"), "concrete/concretefloor001a");
        assert!(is_normalized_material("concrete/concretefloor001a"));
        assert!(!is_normalized_material("CONCRETE/x"));
        assert!(!is_normalized_material("materials/x"));
        assert!(!is_normalized_material(""));
    }

    #[test]
    fn model_paths() {
        assert_eq!(model_key("Models\\Props_C17\\OilDrum001.MDL"), "models/props_c17/oildrum001.mdl");
        assert!(matches!(model_key_cow("models/a.mdl"), Cow::Borrowed(_)));
        assert_eq!(model_key_cow("Models/A.mdl"), "models/a.mdl");
        assert!(is_model_path("models/a.MDL"));
        assert!(!is_model_path("editor/light.vmt"));
        assert!(!is_model_path(".mdl"));
    }

    #[test]
    fn sprites() {
        assert_eq!(sprite_material("editor/light.vmt").as_deref(), Some("editor/light"));
        assert_eq!(sprite_material("materials/Sprites/Glow01.vmt").as_deref(), Some("sprites/glow01"));
        assert_eq!(sprite_material("sprites/glow01.spr").as_deref(), Some("sprites/glow01"));
        assert_eq!(sprite_material("models/a.mdl"), None);
        assert_eq!(sprite_material("  "), None);
    }

    #[test]
    fn stems() {
        assert_eq!(sanitize_stem("My Brick Wall").as_deref(), Some("my_brick_wall"));
        assert_eq!(sanitize_stem("Été déjà").as_deref(), Some("ete_deja"));
        assert_eq!(sanitize_stem("__"), None);
        assert_eq!(normalize_prefix("Materials/Custom/Bricks/"), "custom/bricks");
    }

    #[test]
    fn floats_and_colors() {
        assert_eq!(parse_floats::<4>("255 128 0 200"), ([255.0, 128.0, 0.0, 200.0], 4));
        assert_eq!(parse_floats::<4>("1 2 x"), ([1.0, 2.0, 0.0, 0.0], 2));
    }
}
