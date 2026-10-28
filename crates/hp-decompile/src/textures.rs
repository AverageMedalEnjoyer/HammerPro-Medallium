fn strip_cubemap_suffix(name: &str) -> Option<&str> {
    let mut rest = name;
    for _ in 0..3 {
        let cut = rest.rfind('_')?;
        let num = &rest[cut + 1..];
        let digits = num.strip_prefix('-').unwrap_or(num);
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        rest = &rest[..cut];
    }
    (!rest.is_empty()).then_some(rest)
}

pub fn original_material(name: &str, map: &str) -> (String, bool) {
    let normalized = name.replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    let prefix = format!("maps/{}/", map.to_ascii_lowercase());
    let Some(rest) = (!map.is_empty() && lower.starts_with(&prefix)).then(|| &normalized[prefix.len()..]) else {
        return (name.to_owned(), false);
    };
    let lower_rest = rest.to_ascii_lowercase();
    let stripped = if let Some(base) = lower_rest.strip_suffix("_wvt_patch") {
        &rest[..base.len()]
    } else if let Some(cut) = lower_rest.rfind("_depth_")
        && lower_rest[cut + 7..].bytes().all(|b| b.is_ascii_digit())
        && cut + 7 < lower_rest.len()
    {
        &rest[..cut]
    } else if let Some(base) = strip_cubemap_suffix(rest) {
        base
    } else {
        return (name.to_owned(), false);
    };
    if stripped.is_empty() {
        return (name.to_owned(), false);
    }
    (stripped.to_owned(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patched_names_come_back() {
        let map = "gm_test";
        assert_eq!(
            original_material("maps/gm_test/concrete/concretefloor001a_-128_256_64", map),
            ("concrete/concretefloor001a".to_owned(), true)
        );
        assert_eq!(
            original_material("MAPS/GM_TEST/Nature/Blendgrass_wvt_patch", map),
            ("Nature/Blendgrass".to_owned(), true)
        );
        assert_eq!(original_material("maps/gm_test/water/water_depth_64", map), ("water/water".to_owned(), true));
        assert_eq!(original_material("concrete/a_1_2_3", map), ("concrete/a_1_2_3".to_owned(), false));
        assert_eq!(original_material("maps/other/a_1_2_3", map), ("maps/other/a_1_2_3".to_owned(), false));
        assert_eq!(original_material("maps/gm_test/brick_1_2", map), ("maps/gm_test/brick_1_2".to_owned(), false));
        assert!(!original_material("maps/gm_test/", map).1);
        assert!(!original_material("anything", "").1);
    }
}
