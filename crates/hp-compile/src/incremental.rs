use serde::{Deserialize, Serialize};

use crate::plan::{BspMode, RadMode, StepModes, VisMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Fingerprint {
    pub structural: u64,
    pub detail: u64,
    pub bake: u64,
    pub static_props: u64,
    pub detail_props: u64,
    pub logic: u64,
    pub brush_models: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BuildRecord {
    pub fingerprint: Fingerprint,
    pub context: u64,
    pub bsp_size: u64,
    pub bsp_mtime_ns: u64,
}

impl BuildRecord {
    pub fn encode(&self) -> String {
        let f = &self.fingerprint;
        let words = [
            f.structural,
            f.detail,
            f.bake,
            f.static_props,
            f.detail_props,
            f.logic,
            f.brush_models,
            self.context,
            self.bsp_size,
            self.bsp_mtime_ns,
        ];
        let mut out = String::from("v1");
        for w in words {
            out.push(' ');
            out.push_str(&format!("{w:x}"));
        }
        out
    }

    pub fn decode(text: &str) -> Option<BuildRecord> {
        let mut parts = text.split_ascii_whitespace();
        if parts.next()? != "v1" {
            return None;
        }
        let mut w = [0u64; 10];
        for slot in &mut w {
            *slot = u64::from_str_radix(parts.next()?, 16).ok()?;
        }
        if parts.next().is_some() {
            return None;
        }
        let [
            structural,
            detail,
            bake,
            static_props,
            detail_props,
            logic,
            brush_models,
            context,
            bsp_size,
            bsp_mtime_ns,
        ] = w;
        Some(BuildRecord {
            fingerprint: Fingerprint { structural, detail, bake, static_props, detail_props, logic, brush_models },
            context,
            bsp_size,
            bsp_mtime_ns,
        })
    }
}

pub fn file_stamp(path: &std::path::Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some((meta.len(), u64::try_from(mtime.as_nanos()).unwrap_or(u64::MAX)))
}

pub fn context_hash<'a>(parts: impl IntoIterator<Item = &'a str>) -> u64 {
    let mut h = crate::stable_hash::StableHasher::new();
    for p in parts {
        h.str(p);
    }
    h.finish_u64()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FullReason {
    NoPreviousBuild,
    SettingsChanged,
    BspChanged,
    GeometryChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AutoKind {
    UpToDate,
    EntitiesOnly,
    PropsOnly,
    LightingOnly,
    DetailProps,
    Full(FullReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Choice {
    pub modes: StepModes,
    pub kind: AutoKind,
}

pub fn choose(
    prev: Option<&BuildRecord>,
    now: &Fingerprint,
    base: StepModes,
    context: u64,
    bsp: Option<(u64, u64)>,
) -> Choice {
    let full = |reason| Choice { modes: base, kind: AutoKind::Full(reason) };
    let Some(prev) = prev else { return full(FullReason::NoPreviousBuild) };
    if prev.context != context {
        return full(FullReason::SettingsChanged);
    }
    if bsp != Some((prev.bsp_size, prev.bsp_mtime_ns)) {
        return full(FullReason::BspChanged);
    }
    let p = &prev.fingerprint;
    if p.structural != now.structural || p.detail != now.detail || p.brush_models != now.brush_models {
        return full(FullReason::GeometryChanged);
    }
    let logic = p.logic != now.logic;
    let bake = p.bake != now.bake;
    let statics = p.static_props != now.static_props;
    let details = p.detail_props != now.detail_props;
    let relight = if base.rad == RadMode::Off { RadMode::Draft } else { base.rad };
    let entities = logic || bake;
    let (bsp, rad, kind) = if details {
        let bsp = if entities { BspMode::EntsAndProps } else { BspMode::OnlyProps };
        (bsp, relight, if bake { AutoKind::LightingOnly } else { AutoKind::DetailProps })
    } else if bake {
        (BspMode::OnlyEnts, relight, AutoKind::LightingOnly)
    } else if logic {
        (BspMode::OnlyEnts, RadMode::Off, if statics { AutoKind::PropsOnly } else { AutoKind::EntitiesOnly })
    } else if statics {
        (BspMode::OnlyProps, RadMode::Off, AutoKind::PropsOnly)
    } else {
        (BspMode::Off, RadMode::Off, AutoKind::UpToDate)
    };
    Choice { modes: StepModes { bsp, vis: VisMode::Off, rad }, kind }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::Preset;

    fn record(fp: Fingerprint) -> BuildRecord {
        BuildRecord { fingerprint: fp, context: 7, bsp_size: 100, bsp_mtime_ns: 5 }
    }

    const BSP: Option<(u64, u64)> = Some((100, 5));

    #[test]
    fn decision_table() {
        let base = Preset::Draft.modes();
        let fp = Fingerprint {
            structural: 1,
            detail: 2,
            bake: 3,
            static_props: 4,
            detail_props: 5,
            logic: 6,
            brush_models: 7,
        };
        let prev = record(fp);
        let pick = |now: Fingerprint| choose(Some(&prev), &now, base, 7, BSP);

        let same = pick(fp);
        assert_eq!(same.kind, AutoKind::UpToDate);
        assert_eq!(same.modes, StepModes::new(BspMode::Off, VisMode::Off, RadMode::Off));

        let logic = pick(Fingerprint { logic: 60, ..fp });
        assert_eq!(
            (logic.kind, logic.modes.bsp, logic.modes.rad),
            (AutoKind::EntitiesOnly, BspMode::OnlyEnts, RadMode::Off)
        );
        assert_eq!(logic.modes.vis, VisMode::Off);

        let props = pick(Fingerprint { static_props: 40, ..fp });
        assert_eq!((props.kind, props.modes.bsp), (AutoKind::PropsOnly, BspMode::OnlyProps));
        let props_logic = pick(Fingerprint { static_props: 40, logic: 60, ..fp });
        assert_eq!((props_logic.kind, props_logic.modes.bsp), (AutoKind::PropsOnly, BspMode::OnlyEnts));

        let lights = pick(Fingerprint { bake: 30, ..fp });
        assert_eq!(
            (lights.kind, lights.modes.bsp, lights.modes.rad),
            (AutoKind::LightingOnly, BspMode::OnlyEnts, RadMode::Draft)
        );

        let details = pick(Fingerprint { detail_props: 50, ..fp });
        assert_eq!(
            (details.kind, details.modes.bsp, details.modes.rad),
            (AutoKind::DetailProps, BspMode::OnlyProps, RadMode::Draft)
        );
        let details_logic = pick(Fingerprint { detail_props: 50, logic: 1, ..fp });
        assert_eq!(details_logic.modes.bsp, BspMode::EntsAndProps);
        let details_lights = pick(Fingerprint { detail_props: 50, bake: 1, ..fp });
        assert_eq!((details_lights.modes.bsp, details_lights.modes.rad), (BspMode::EntsAndProps, RadMode::Draft));

        for now in [
            Fingerprint { structural: 10, ..fp },
            Fingerprint { detail: 20, logic: 1, ..fp },
            Fingerprint { brush_models: 70, ..fp },
        ] {
            let c = pick(now);
            assert_eq!(c.kind, AutoKind::Full(FullReason::GeometryChanged));
            assert_eq!(c.modes, base);
        }

        let normal = choose(Some(&prev), &Fingerprint { bake: 30, ..fp }, Preset::Normal.modes(), 7, BSP);
        assert_eq!(normal.modes.rad, RadMode::Normal);
    }

    #[test]
    fn invalidation() {
        let fp = Fingerprint::default();
        let base = Preset::Normal.modes();
        assert_eq!(choose(None, &fp, base, 7, BSP).kind, AutoKind::Full(FullReason::NoPreviousBuild));
        let prev = record(fp);
        assert_eq!(choose(Some(&prev), &fp, base, 8, BSP).kind, AutoKind::Full(FullReason::SettingsChanged));
        assert_eq!(choose(Some(&prev), &fp, base, 7, None).kind, AutoKind::Full(FullReason::BspChanged));
        assert_eq!(choose(Some(&prev), &fp, base, 7, Some((100, 6))).kind, AutoKind::Full(FullReason::BspChanged));
        assert_eq!(context_hash(["a", "b"]), context_hash(["a", "b"]));
        let r = BuildRecord {
            fingerprint: Fingerprint { structural: u64::MAX, detail: 1, ..Fingerprint::default() },
            context: 0xABCD,
            bsp_size: 10,
            bsp_mtime_ns: 99,
        };
        assert_eq!(BuildRecord::decode(&r.encode()), Some(r.clone()));
        assert_eq!(BuildRecord::decode("v2 1 2"), None);
        assert_eq!(BuildRecord::decode("v1 1 2 3"), None);
        assert_eq!(BuildRecord::decode(&format!("{} 5", r.encode())), None);
        assert_eq!(BuildRecord::decode("v1 zz 0 0 0 0 0 0 0 0 0"), None);
        assert_ne!(context_hash(["a", "b"]), context_hash(["ab"]));
        let dir = crate::test_util::scratch("incremental_stamp");
        let f = dir.join("x.bsp");
        assert_eq!(file_stamp(&f), None);
        std::fs::write(&f, b"1234").unwrap();
        assert_eq!(file_stamp(&f).map(|s| s.0), Some(4));
    }
}
