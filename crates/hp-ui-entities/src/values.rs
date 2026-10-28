use glam::DVec3;

pub fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_owned();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        let i = v as i64;
        return i.to_string();
    }
    let s = format!("{v}");
    if s == "-0" { "0".to_owned() } else { s }
}

pub fn fmt_num_rounded(v: f64, decimals: i32) -> String {
    let k = 10f64.powi(decimals);
    fmt_num((v * k).round() / k)
}

pub fn parse_numbers(s: &str) -> Option<Vec<f64>> {
    let s = s.trim().trim_start_matches(['(', '[', '{']).trim_end_matches([')', ']', '}']);
    s.split_ascii_whitespace().map(|t| t.parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color255 {
    pub rgb: [f64; 3],
    pub brightness: Option<f64>,
}

impl Color255 {
    pub fn parse(s: &str) -> Option<Color255> {
        let n = parse_numbers(s)?;
        match n.as_slice() {
            [r, g, b] => Some(Color255 { rgb: [*r, *g, *b], brightness: None }),
            [r, g, b, i, ..] => Some(Color255 { rgb: [*r, *g, *b], brightness: Some(*i) }),
            _ => None,
        }
    }

    pub fn is_unset(&self) -> bool {
        self.rgb.iter().any(|c| *c < 0.0)
    }

    pub fn to_srgb(&self) -> [u8; 3] {
        self.rgb.map(|c| c.round().clamp(0.0, 255.0) as u8)
    }

    pub fn with_srgb(self, rgb: [u8; 3]) -> Color255 {
        Color255 { rgb: rgb.map(f64::from), brightness: self.brightness }
    }

    pub fn format(&self) -> String {
        let mut s = format!("{} {} {}", fmt_num(self.rgb[0]), fmt_num(self.rgb[1]), fmt_num(self.rgb[2]));
        if let Some(i) = self.brightness {
            s.push(' ');
            s.push_str(&fmt_num(i));
        }
        s
    }
}

pub fn parse_color1(s: &str) -> Option<[f32; 3]> {
    let n = parse_numbers(s)?;
    match n.as_slice() {
        [r, g, b, ..] => Some([*r as f32, *g as f32, *b as f32]),
        _ => None,
    }
}

pub fn format_color1(c: [f32; 3]) -> String {
    format!(
        "{} {} {}",
        fmt_num_rounded(f64::from(c[0]), 4),
        fmt_num_rounded(f64::from(c[1]), 4),
        fmt_num_rounded(f64::from(c[2]), 4)
    )
}

pub fn parse_angles(s: &str) -> Option<DVec3> {
    let n = parse_numbers(s)?;
    match n.as_slice() {
        [y] => Some(DVec3::new(0.0, *y, 0.0)),
        [p, y, r, ..] => Some(DVec3::new(*p, *y, *r)),
        _ => None,
    }
}

pub fn parse_pitch(s: &str) -> Option<f64> {
    match parse_numbers(s)?.as_slice() {
        [p] => Some(*p),
        _ => None,
    }
}

pub fn format_angles(a: DVec3) -> String {
    format!("{} {} {}", fmt_num_rounded(a.x, 4), fmt_num_rounded(a.y, 4), fmt_num_rounded(a.z, 4))
}

pub fn normalize_yaw(yaw: f64) -> f64 {
    let y = yaw.rem_euclid(360.0);
    if (y - 360.0).abs() < 1e-9 { 0.0 } else { y }
}

pub fn yaw_from_screen_dir(dx: f32, dy: f32, snap: Option<f64>) -> Option<f64> {
    if dx.abs() < 1e-3 && dy.abs() < 1e-3 {
        return None;
    }
    let yaw = f64::from(-dy).atan2(f64::from(dx)).to_degrees();
    let yaw = match snap {
        Some(step) if step > 0.0 => (yaw / step).round() * step,
        _ => yaw.round(),
    };
    Some(normalize_yaw(yaw))
}

pub fn parse_vec3(s: &str) -> Option<DVec3> {
    let n = parse_numbers(s)?;
    match n.as_slice() {
        [x, y, z] => Some(DVec3::new(*x, *y, *z)),
        _ => None,
    }
}

pub fn format_vec3(v: DVec3) -> String {
    format!("{} {} {}", fmt_num_rounded(v.x, 4), fmt_num_rounded(v.y, 4), fmt_num_rounded(v.z, 4))
}

pub fn parse_axis(s: &str) -> Option<[DVec3; 2]> {
    let (a, b) = s.split_once(',')?;
    Some([parse_vec3(a)?, parse_vec3(b)?])
}

pub fn format_axis(points: [DVec3; 2]) -> String {
    format!("{}, {}", format_vec3(points[0]), format_vec3(points[1]))
}

pub fn is_valid_integer(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.parse::<i64>().is_ok()
}

pub fn is_valid_float(s: &str) -> bool {
    let t = s.trim();
    t.is_empty() || t.parse::<f64>().is_ok_and(f64::is_finite)
}

pub fn values_equal(a: &str, b: &str) -> bool {
    let (a, b) = (a.trim(), b.trim());
    if a.eq_ignore_ascii_case(b) {
        return true;
    }
    match (parse_numbers(a), parse_numbers(b)) {
        (Some(x), Some(y)) if !x.is_empty() && x.len() == y.len() => {
            x.iter().zip(&y).all(|(p, q)| (p - q).abs() < 1e-6)
        }
        _ => false,
    }
}

pub fn parse_flags(s: &str) -> u32 {
    let t = s.trim();
    if let Ok(v) = t.parse::<i64>() {
        return v as u32;
    }
    if let Ok(v) = t.parse::<f64>()
        && v.is_finite()
    {
        return (v.trunc() as i64) as u32;
    }
    let end = t
        .char_indices()
        .find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+'))))
        .map_or(t.len(), |(i, _)| i);
    t.get(..end).and_then(|p| p.parse::<i64>().ok()).map_or(0, |v| v as u32)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tri {
    Off,
    On,
    Mixed,
}

impl Tri {
    pub fn of_bit(values: &[u32], bit: u32) -> Tri {
        let on = values.iter().filter(|v| **v & bit != 0).count();
        if on == 0 {
            Tri::Off
        } else if on == values.len() {
            Tri::On
        } else {
            Tri::Mixed
        }
    }

    pub fn of_bools(values: impl IntoIterator<Item = bool>) -> Tri {
        let (mut any_on, mut any_off) = (false, false);
        for v in values {
            if v {
                any_on = true;
            } else {
                any_off = true;
            }
        }
        match (any_on, any_off) {
            (true, false) => Tri::On,
            (true, true) => Tri::Mixed,
            _ => Tri::Off,
        }
    }

    pub fn toggled(self) -> bool {
        self != Tri::On
    }
}

pub fn with_bit(value: u32, bit: u32, on: bool) -> u32 {
    if on { value | bit } else { value & !bit }
}

pub fn bits_of(value: u32) -> impl Iterator<Item = u32> {
    (0..32).map(|i| 1u32 << i).filter(move |b| value & b != 0)
}

pub fn parse_bool(s: &str) -> bool {
    let t = s.trim();
    match t.parse::<f64>() {
        Ok(v) => v != 0.0,
        Err(_) => t.eq_ignore_ascii_case("true") || t.eq_ignore_ascii_case("yes"),
    }
}

pub fn choice_index(choices: &[hp_fgd::Choice], value: &str) -> Option<usize> {
    let v = value.trim();
    let num = v.parse::<f64>().ok();
    choices.iter().position(|c| {
        let cv = c.value.trim();
        match (num, cv.parse::<f64>().ok()) {
            (Some(a), Some(b)) => a == b,
            _ => cv.eq_ignore_ascii_case(v),
        }
    })
}

pub fn preview_material_name(value: &str) -> String {
    let mut s = value.trim().replace('\\', "/").to_ascii_lowercase();
    if let Some(rest) = s.strip_prefix("materials/") {
        s = rest.to_owned();
    }
    for ext in [".vmt", ".vtf", ".spr"] {
        if let Some(rest) = s.strip_suffix(ext) {
            s = rest.to_owned();
            break;
        }
    }
    s
}

pub fn sound_key_name(asset: &str) -> &str {
    asset.strip_prefix("sound/").unwrap_or(asset)
}

pub fn sky_names(materials: &[hp_fs::AssetEntry]) -> Vec<String> {
    let mut out: Vec<String> = materials
        .iter()
        .filter_map(|m| m.name.strip_prefix("skybox/")?.strip_suffix("rt"))
        .filter(|n| !n.is_empty() && !n.contains('/'))
        .map(str::to_owned)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

pub fn model_folder(path: &str) -> &str {
    path.rfind('/').map_or("", |i| &path[..i])
}

pub fn model_file_stem(path: &str) -> &str {
    let name = path.rfind('/').map_or(path, |i| &path[i + 1..]);
    name.strip_suffix(".mdl").unwrap_or(name)
}

pub fn model_top_folder(path: &str) -> &str {
    let rest = path.strip_prefix("models/").unwrap_or(path);
    match rest.find('/') {
        Some(i) => &rest[..i],
        None => "",
    }
}

pub fn model_file_path(value: &str) -> Option<String> {
    let path = hp_fs::normalize_path(value.trim()).trim_start_matches('/').to_owned();
    if path.is_empty() {
        return None;
    }
    let sprite = path.strip_suffix(".spr").or_else(|| path.strip_suffix(".vmt"));
    Some(match sprite {
        Some(stem) => {
            let stem = stem.strip_prefix("materials/").unwrap_or(stem);
            format!("materials/{stem}.vmt")
        }
        None => path,
    })
}

pub fn is_missing_model(fs: Option<&hp_fs::GameFs>, value: &str) -> bool {
    match (fs, model_file_path(value)) {
        (Some(fs), Some(path)) => !fs.exists(&path),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_format_like_hammer() {
        assert_eq!(fmt_num(300.0), "300");
        assert_eq!(fmt_num(-0.0), "0");
        assert_eq!(fmt_num(0.25), "0.25");
        assert_eq!(fmt_num(f64::NAN), "0");
        assert_eq!(fmt_num_rounded(89.99999999, 4), "90");
        assert_eq!(fmt_num_rounded(-0.00001, 4), "0");
    }

    #[test]
    fn color255_with_and_without_brightness() {
        let c = Color255::parse("255 240 200 300").unwrap();
        assert_eq!(c.rgb, [255.0, 240.0, 200.0]);
        assert_eq!(c.brightness, Some(300.0));
        assert_eq!(c.format(), "255 240 200 300");
        let c2 = c.with_srgb([10, 20, 30]);
        assert_eq!(c2.format(), "10 20 30 300");

        let plain = Color255::parse(" 12 34 56 ").unwrap();
        assert_eq!(plain.brightness, None);
        assert_eq!(plain.format(), "12 34 56");
        assert_eq!(plain.to_srgb(), [12, 34, 56]);

        let hdr = Color255::parse("-1 -1 -1 1").unwrap();
        assert!(hdr.is_unset());
        assert_eq!(hdr.to_srgb(), [0, 0, 0]);
        assert_eq!(hdr.format(), "-1 -1 -1 1");

        let float = Color255::parse("255 128.5 0 50.25").unwrap();
        assert_eq!(float.format(), "255 128.5 0 50.25");
        assert_eq!(float.to_srgb(), [255, 129, 0]);

        assert!(Color255::parse("255 255").is_none());
        assert!(Color255::parse("red").is_none());
        assert!(Color255::parse("").is_none());
    }

    #[test]
    fn color1_round_trip() {
        let c = parse_color1("1 0.5 0.25").unwrap();
        assert_eq!(c, [1.0, 0.5, 0.25]);
        assert_eq!(format_color1(c), "1 0.5 0.25");
        assert_eq!(format_color1([0.333333, 0.0, 1.0]), "0.3333 0 1");
        assert!(parse_color1("1 1").is_none());
    }

    #[test]
    fn angles_parse_and_format() {
        assert_eq!(parse_angles("0 90 0"), Some(DVec3::new(0.0, 90.0, 0.0)));
        assert_eq!(parse_angles("-45 180.5 10"), Some(DVec3::new(-45.0, 180.5, 10.0)));
        assert_eq!(parse_angles("270"), Some(DVec3::new(0.0, 270.0, 0.0)));
        assert_eq!(parse_angles("1 2"), None);
        assert_eq!(parse_angles("a b c"), None);
        assert_eq!(format_angles(DVec3::new(0.0, 89.999999999, -0.0)), "0 90 0");
        assert_eq!(normalize_yaw(-90.0), 270.0);
        assert_eq!(normalize_yaw(720.0), 0.0);
        assert_eq!(normalize_yaw(359.9999999999), 0.0);
    }

    #[test]
    fn model_file_paths() {
        assert_eq!(
            model_file_path(r" Models\Props_C17\OilDrum001.mdl ").as_deref(),
            Some("models/props_c17/oildrum001.mdl")
        );
        assert_eq!(model_file_path("/models/a.mdl").as_deref(), Some("models/a.mdl"));
        assert_eq!(model_file_path("sprites/Glow01.spr").as_deref(), Some("materials/sprites/glow01.vmt"));
        assert_eq!(model_file_path("materials/sprites/glow01.vmt").as_deref(), Some("materials/sprites/glow01.vmt"));
        assert_eq!(model_file_path("models/no_extension").as_deref(), Some("models/no_extension"));
        assert_eq!(model_file_path("  "), None);
    }

    #[test]
    fn missing_models_are_looked_up_in_the_mounted_content() {
        let dir = std::env::temp_dir().join(format!("hp-ui-entities-missing-model-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(dir.join("models").join("props"));
        let _ = std::fs::create_dir_all(dir.join("materials").join("sprites"));
        let _ = std::fs::write(dir.join("models").join("props").join("Crate.mdl"), b"IDST");
        let _ = std::fs::write(dir.join("materials").join("sprites").join("glow01.vmt"), b"\"Sprite\" {}");
        let fs = hp_fs::GameFs::empty();
        assert!(fs.add_game_dir(&dir).is_ok());

        assert!(!is_missing_model(Some(&fs), "models/props/crate.mdl"));
        assert!(!is_missing_model(Some(&fs), r"MODELS\Props\CRATE.MDL"), "lookups ignore case and separators");
        assert!(!is_missing_model(Some(&fs), "sprites/glow01.spr"));
        assert!(is_missing_model(Some(&fs), "models/props/crate2.mdl"));
        assert!(is_missing_model(Some(&fs), "models/props/crate"), "the engine needs the extension");
        assert!(!is_missing_model(Some(&fs), ""), "an empty key is not a missing model");
        assert!(!is_missing_model(None, "models/props/crate2.mdl"), "unknown until the content is mounted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn negative_pitch_is_one_number() {
        assert_eq!(parse_pitch("-90"), Some(-90.0));
        assert_eq!(parse_pitch(" 45.5 "), Some(45.5));
        assert_eq!(parse_pitch("0 -45 0"), None, "three components are not a pitch");
        assert_eq!(parse_pitch(""), None);
        assert_eq!(parse_pitch("down"), None);
    }

    #[test]
    fn yaw_from_screen_directions() {
        assert_eq!(yaw_from_screen_dir(10.0, 0.0, None), Some(0.0));
        assert_eq!(yaw_from_screen_dir(0.0, -10.0, None), Some(90.0));
        assert_eq!(yaw_from_screen_dir(-10.0, 0.0, None), Some(180.0));
        assert_eq!(yaw_from_screen_dir(0.0, 10.0, None), Some(270.0));
        assert_eq!(yaw_from_screen_dir(10.0, -9.0, Some(15.0)), Some(45.0));
        assert_eq!(yaw_from_screen_dir(0.0, 0.0, None), None);
    }

    #[test]
    fn axes() {
        let a = parse_axis("0 0 0, 0 0 64").unwrap();
        assert_eq!(a, [DVec3::ZERO, DVec3::new(0.0, 0.0, 64.0)]);
        assert_eq!(format_axis(a), "0 0 0, 0 0 64");
        assert_eq!(parse_axis("1 2 3"), None);
        assert_eq!(parse_axis("1 2, 3 4 5"), None);
        assert_eq!(parse_axis(","), None);
    }

    #[test]
    fn vectors() {
        assert_eq!(parse_vec3("1 2 3"), Some(DVec3::new(1.0, 2.0, 3.0)));
        assert_eq!(parse_vec3("[1 2 3]"), Some(DVec3::new(1.0, 2.0, 3.0)));
        assert_eq!(parse_vec3("1 2"), None);
        assert_eq!(format_vec3(DVec3::new(0.5, -16.0, 1e-9)), "0.5 -16 0");
    }

    #[test]
    fn numeric_validation() {
        assert!(is_valid_integer("42"));
        assert!(is_valid_integer(" -1 "));
        assert!(is_valid_integer(""));
        assert!(!is_valid_integer("1.5"));
        assert!(!is_valid_integer("abc"));
        assert!(is_valid_float("1.5"));
        assert!(is_valid_float("-3"));
        assert!(!is_valid_float("nan"));
        assert!(!is_valid_float("1,5"));
    }

    #[test]
    fn value_equality() {
        assert!(values_equal("1", "1.0"));
        assert!(values_equal("255 255 255 200", "255 255 255 200.0"));
        assert!(values_equal("Abc", "aBC"));
        assert!(!values_equal("1 2", "1 2 3"));
        assert!(!values_equal("0", ""));
    }

    #[test]
    fn flags_parse_and_merge() {
        assert_eq!(parse_flags("1"), 1);
        assert_eq!(parse_flags("4097"), 4097);
        assert_eq!(parse_flags("-1"), u32::MAX);
        assert_eq!(parse_flags("3.0"), 3);
        assert_eq!(parse_flags("12abc"), 12);
        assert_eq!(parse_flags("junk"), 0);
        assert_eq!(parse_flags(""), 0);

        let values = [1u32, 3, 5];
        assert_eq!(Tri::of_bit(&values, 1), Tri::On);
        assert_eq!(Tri::of_bit(&values, 2), Tri::Mixed);
        assert_eq!(Tri::of_bit(&values, 8), Tri::Off);
        assert!(Tri::Mixed.toggled());
        assert!(Tri::Off.toggled());
        assert!(!Tri::On.toggled());
        assert_eq!(with_bit(1, 4, true), 5);
        assert_eq!(with_bit(5, 1, false), 4);
        assert_eq!(bits_of(0b1010).collect::<Vec<_>>(), [2, 8]);
        assert_eq!(Tri::of_bools([true, true]), Tri::On);
        assert_eq!(Tri::of_bools([true, false]), Tri::Mixed);
        assert_eq!(Tri::of_bools([]), Tri::Off);
    }

    #[test]
    fn choices_map_values() {
        let choices = vec![
            hp_fgd::Choice { value: "0".into(), display: "No".into() },
            hp_fgd::Choice { value: "1".into(), display: "Yes".into() },
            hp_fgd::Choice { value: "models/a.mdl".into(), display: "A".into() },
        ];
        assert_eq!(choice_index(&choices, "1"), Some(1));
        assert_eq!(choice_index(&choices, "1.0"), Some(1));
        assert_eq!(choice_index(&choices, "MODELS/A.MDL"), Some(2));
        assert_eq!(choice_index(&choices, "2"), None);
        assert!(parse_bool("1"));
        assert!(!parse_bool("0"));
        assert!(parse_bool("true"));
        assert!(!parse_bool(""));
    }

    #[test]
    fn asset_names() {
        assert_eq!(preview_material_name("Materials\\Sprites\\Glow01.VMT"), "sprites/glow01");
        assert_eq!(preview_material_name("sprites/light_glow02_add_noz"), "sprites/light_glow02_add_noz");
        assert_eq!(preview_material_name("sprites/zerogxplode.spr"), "sprites/zerogxplode");
        assert_eq!(sound_key_name("sound/ambient/wind/wind1.wav"), "ambient/wind/wind1.wav");
        assert_eq!(sound_key_name("ambient/wind/wind1.wav"), "ambient/wind/wind1.wav");
        let mats: Vec<hp_fs::AssetEntry> =
            ["skybox/sky_day01_01rt", "skybox/sky_day01_01lf", "skybox/sky_borealis01rt", "brick/rt"]
                .iter()
                .map(|n| hp_fs::AssetEntry { name: (*n).to_owned(), source: 0 })
                .collect();
        assert_eq!(sky_names(&mats), ["sky_borealis01", "sky_day01_01"]);
        assert_eq!(model_folder("models/props_c17/oildrum001.mdl"), "models/props_c17");
        assert_eq!(model_folder("x.mdl"), "");
        assert_eq!(model_file_stem("models/props_c17/oildrum001.mdl"), "oildrum001");
        assert_eq!(model_top_folder("models/props_c17/sub/a.mdl"), "props_c17");
        assert_eq!(model_top_folder("models/a.mdl"), "");
    }
}
