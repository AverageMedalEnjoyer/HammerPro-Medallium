use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize)]
pub enum Severity {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct LogLocation {
    pub brush_id: Option<u32>,
    pub side_id: Option<u32>,
    pub entity_id: Option<u32>,
    pub entity_class: Option<String>,
    pub position: Option<[f64; 3]>,
}

impl LogLocation {
    pub fn is_empty(&self) -> bool {
        self.brush_id.is_none()
            && self.side_id.is_none()
            && self.entity_id.is_none()
            && self.entity_class.is_none()
            && self.position.is_none()
    }
}

#[derive(Debug, Clone, Copy)]
enum Match {
    Contains(&'static str),
    EndsWith(&'static str),
    IdPrefix(&'static str),
}

#[derive(Debug, Clone, Copy)]
struct Rule {
    pattern: Match,
    severity: Severity,
}

const fn rule(pattern: Match, severity: Severity) -> Rule {
    Rule { pattern, severity }
}

use Match::{Contains, EndsWith, IdPrefix};
use Severity::{Error, Success, Warning};

const RULES: &[Rule] = &[
    rule(Contains("**** leaked ****"), Error),
    rule(Contains("map leaked"), Error),
    rule(Contains("leaked!"), Error),
    rule(Contains("areaportal leak"), Error),
    rule(Contains("leaked"), Error),
    rule(Contains("of the engine limit, over it"), Error),
    rule(Contains("of the engine limit ("), Warning),
    rule(Contains("max_map_"), Error),
    rule(Contains("fatal"), Error),
    rule(Contains("out of memory"), Error),
    rule(Contains("exceeded"), Error),
    rule(Contains("to build the default cubemap"), Warning),
    rule(Contains("skybox vtf files for"), Warning),
    rule(Contains("warning"), Warning),
    rule(Contains("error"), Error),
    rule(Contains("too many"), Error),
    rule(Contains("too much"), Error),
    rule(Contains("overflow"), Error),
    rule(Contains("findportalside"), Warning),
    rule(Contains("candidate brush ids"), Warning),
    rule(Contains("_zero_percent_distance"), Warning),
    rule(Contains("_fifty_percent_distance"), Warning),
    rule(Contains("using default"), Warning),
    rule(Contains("material not found"), Warning),
    rule(Contains("has no texture"), Warning),
    rule(Contains("is missing"), Warning),
    rule(Contains("could not"), Error),
    rule(Contains("couldn't"), Error),
    rule(Contains("can't"), Error),
    rule(Contains("cannot"), Error),
    rule(Contains("unable to"), Error),
    rule(Contains("failed"), Error),
    rule(Contains("failure"), Error),
    rule(Contains("not supported"), Error),
    rule(Contains("not allowed"), Error),
    rule(Contains("bounds out of range"), Error),
    rule(Contains("bad surface extents"), Error),
    rule(Contains("bad normal"), Error),
    rule(Contains("non-quad displacement"), Error),
    rule(Contains("in solid volume"), Error),
    rule(Contains("doesn't touch two areas"), Error),
    rule(Contains("can only be a single brush"), Error),
    rule(Contains("outside the map"), Warning),
    rule(Contains("microbrush"), Warning),
    rule(Contains("duplicate plane"), Warning),
    rule(Contains("mirrored plane"), Warning),
    rule(Contains("plane with no normal"), Warning),
    rule(Contains("mixed face contents"), Warning),
    rule(Contains("texture axis perpendicular"), Warning),
    rule(Contains("no visible sides"), Warning),
    rule(Contains("bad geometry"), Warning),
    rule(Contains("huge winding"), Warning),
    rule(Contains("unbounded volume"), Warning),
    rule(Contains("missing target"), Warning),
    rule(Contains("pointing at deleted"), Warning),
    rule(IdPrefix("brush"), Warning),
    rule(IdPrefix("entity"), Warning),
    rule(IdPrefix("side"), Warning),
    rule(EndsWith("elapsed"), Success),
];

pub fn parse_log_line(line: &str) -> (Severity, Option<LogLocation>) {
    let trimmed = line.trim();
    if trimmed.is_empty() || is_stat_line(trimmed) || is_statistics_item(trimmed) {
        return (Severity::Info, None);
    }
    let lower = trimmed.to_ascii_lowercase();
    let severity = RULES.iter().find(|r| rule_matches(r.pattern, &lower)).map_or(Severity::Info, |r| r.severity);
    if matches!(severity, Severity::Info | Severity::Success) {
        return (severity, None);
    }
    let location = extract_location(trimmed, &lower);
    (severity, (!location.is_empty()).then_some(location))
}

pub fn is_leak_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("**** leaked ****")
        || lower.contains("map leaked")
        || lower.trim_end().ends_with("leaked!")
        || lower.contains("areaportal leak")
}

pub fn describe_exit_code(code: i32) -> Option<&'static str> {
    crate::notice::crash_kind(code).map(crate::notice::CrashKind::english)
}

fn rule_matches(pattern: Match, lower: &str) -> bool {
    match pattern {
        Match::Contains(s) => lower.contains(s),
        Match::EndsWith(s) => lower.ends_with(s),
        Match::IdPrefix(word) => lower.strip_prefix(word).and_then(|rest| rest.strip_prefix(' ')).is_some_and(|rest| {
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            digits > 0 && matches!(rest.as_bytes().get(digits), Some(b':' | b','))
        }),
    }
}

fn is_statistics_item(trimmed: &str) -> bool {
    if trimmed.starts_with("Hint: ") {
        return true;
    }
    let Some(rest) = trimmed.strip_prefix('#') else { return false };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    digits > 0 && rest.as_bytes().get(digits) == Some(&b' ')
}

fn is_stat_line(trimmed: &str) -> bool {
    let mut parts = trimmed.split_ascii_whitespace();
    let (Some(num), Some(word), None) = (parts.next(), parts.next(), parts.next()) else {
        return false;
    };
    num.parse::<i64>().is_ok() && word.bytes().all(|b| b.is_ascii_alphabetic() || b == b'_')
}

fn extract_location(original: &str, lower: &str) -> LogLocation {
    let mut loc = LogLocation {
        brush_id: id_after(lower, "brush"),
        side_id: id_after(lower, "side"),
        entity_id: id_after(lower, "entity"),
        ..LogLocation::default()
    };
    if loc.brush_id.is_none() {
        loc.brush_id = lower
            .find("brush ids:")
            .and_then(|p| lower.get(p + "brush ids:".len()..))
            .and_then(|rest| rest.split_ascii_whitespace().next())
            .and_then(|tok| tok.parse().ok());
    }
    if loc.entity_id.is_none() {
        loc.entity_class = entity_class(original, lower);
    }
    loc.position = find_position(lower);
    loc
}

fn id_after(lower: &str, word: &str) -> Option<u32> {
    let bytes = lower.as_bytes();
    let mut from = 0;
    while let Some(rel) = lower.get(from..).and_then(|s| s.find(word)) {
        let at = from + rel;
        from = at + word.len();
        let boundary = at == 0 || bytes.get(at - 1).is_some_and(|b| !b.is_ascii_alphanumeric() && *b != b'_');
        if !boundary {
            continue;
        }
        let rest = lower.get(at + word.len()..).unwrap_or_default();
        let after_ws = rest.trim_start_matches([' ', '#']);
        if after_ws.len() == rest.len() {
            continue;
        }
        let digits: &str = {
            let n = after_ws.bytes().take_while(u8::is_ascii_digit).count();
            after_ws.get(..n).unwrap_or_default()
        };
        if digits.is_empty() {
            continue;
        }
        if after_ws.as_bytes().get(digits.len()) == Some(&b'/') {
            continue;
        }
        if let Ok(v) = digits.parse() {
            return Some(v);
        }
    }
    None
}

fn entity_class(original: &str, lower: &str) -> Option<String> {
    let at = lower.find("entity ")?;
    let rest = original.get(at + "entity ".len()..)?.trim_start();
    let len = rest.bytes().take_while(|b| b.is_ascii_alphanumeric() || *b == b'_').count();
    let name = rest.get(..len)?;
    if name.is_empty() || name.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let followed_by_paren = rest.get(len..).is_some_and(|r| r.trim_start().starts_with('('));
    (name.contains('_') || followed_by_paren).then(|| name.to_owned())
}

fn find_position(lower: &str) -> Option<[f64; 3]> {
    let mut from = 0;
    while let Some(rel) = lower.get(from..).and_then(|s| s.find('(')) {
        let at = from + rel + 1;
        from = at;
        if let Some(rest) = lower.get(at..)
            && let Some(p) = parse_triple(rest)
        {
            return Some(p);
        }
    }
    for key in [" near ", " at ", " point "] {
        let mut from = 0;
        while let Some(rel) = lower.get(from..).and_then(|s| s.find(key)) {
            let at = from + rel + key.len();
            from = at;
            if let Some(rest) = lower.get(at..)
                && let Some(p) = parse_triple(rest)
            {
                return Some(p);
            }
        }
    }
    None
}

fn parse_triple(s: &str) -> Option<[f64; 3]> {
    let mut out = [0.0f64; 3];
    let mut rest = s.trim_start();
    for (i, slot) in out.iter_mut().enumerate() {
        if i > 0 {
            let trimmed = rest.trim_start_matches([' ', ',', '\t']);
            if trimmed.len() == rest.len() {
                return None;
            }
            rest = trimmed;
        }
        let len = rest.bytes().take_while(|b| b.is_ascii_digit() || matches!(b, b'-' | b'+' | b'.' | b'e')).count();
        let tok = rest.get(..len)?;
        let v: f64 = tok.parse().ok()?;
        if !v.is_finite() {
            return None;
        }
        *slot = v;
        rest = rest.get(len..)?;
    }
    match rest.bytes().next() {
        None | Some(b')' | b' ' | b'!' | b'.' | b',' | b':' | b']') => Some(out),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sev(line: &str) -> Severity {
        parse_log_line(line).0
    }

    fn loc(line: &str) -> LogLocation {
        parse_log_line(line).1.unwrap_or_default()
    }

    #[test]
    fn info_lines() {
        for line in [
            "12 threads",
            "materialPath: D:\\SteamLibrary\\steamapps\\common\\GarrysMod\\garrysmod\\materials",
            "Loading C:\\maps\\ttt_weaponcheck.vmf",
            "fixing up env_cubemap materials on brush sides...",
            "ProcessBlock_Thread: 0...1...2...3...4...5...6...7...8...9...10 (0)",
            "Processing areas...done (0)",
            "Building Faces...done (0)",
            "   4 portalclusters",
            "   12 areaportals",
            "3 areas, 3 areaportals",
            "Loading C:\\maps\\.hammerpro_build\\hammerpro_test_areaportal.vmf",
            "Wrote C:\\maps\\.hammerpro_build\\hammerpro_test_areaportal.bsp",
            "brushes                 13/65535         156/786420   ( 0.0%) ",
            "Reduced 36 texinfos to 5",
            "Wrote ZIP buffer, estimated size 24966, actual size 24886",
            "transfers 512, max 20",
            "Valve Software - vrad.exe SSE (Sep  3 2026) - Garry's Mod Edition (64-bit)",
            "Leaf 0 contents: CONTENTS_SOLID ",
            "#3  12 faces  4.0%  metal/warning_sign_error",
            "#1  brush 12  16 x 16 x 16 units  at (8 8 8)",
            "Hint: brushsides uses 91.6% of the engine limit (MAX_MAP_BRUSHSIDES): reduce it.",
            "LDR lightdata                 [variable]  13000000/16777216  ( 77.5%)",
            "",
        ] {
            assert_eq!(parse_log_line(line), (Severity::Info, None), "{line:?}");
        }
    }

    #[test]
    fn success_lines() {
        assert_eq!(sev("0 seconds elapsed"), Severity::Success);
        assert_eq!(sev("1 minute, 12 seconds elapsed"), Severity::Success);
    }

    #[test]
    fn leak_lines() {
        assert_eq!(sev("**** leaked ****"), Severity::Error);
        assert!(is_leak_line("**** leaked ****"));
        assert!(is_leak_line("--- MAP LEAKED ---"));
        let line = "Entity info_player_start (-680.00 0.00 8.00) leaked!";
        assert!(is_leak_line(line));
        assert_eq!(sev(line), Severity::Error);
        let l = loc(line);
        assert_eq!(l.entity_class.as_deref(), Some("info_player_start"));
        assert_eq!(l.position, Some([-680.0, 0.0, 8.0]));
        assert_eq!(l.entity_id, None);
        assert!(is_leak_line("Areaportal leak ! File: C:\\maps\\a.lin "));
        assert!(!is_leak_line("Writing leaf ambient...done"));
    }

    #[test]
    fn brush_and_side_ids() {
        let l = loc("Brush 1234: WARNING, microbrush");
        assert_eq!(l.brush_id, Some(1234));
        assert_eq!(sev("Brush 1234: WARNING, microbrush"), Severity::Warning);

        let line = "Brush 12, Side 77: texture axis perpendicular to face";
        assert_eq!(sev(line), Severity::Warning);
        let l = loc(line);
        assert_eq!((l.brush_id, l.side_id), (Some(12), Some(77)));

        assert_eq!(sev("Brush 5: bounds out of range"), Severity::Error);
        assert_eq!(loc("Brush 5: bounds out of range").brush_id, Some(5));
        assert_eq!(sev("Brush 9: no visible sides on brush"), Severity::Warning);
        assert_eq!(sev("Brush 3: origin brushes not allowed in world"), Severity::Error);
        assert_eq!(sev("Brush 8: duplicate plane"), Severity::Warning);
        assert_eq!(sev("Brush 8, Side 2: duplicate plane"), Severity::Warning);
        assert_eq!(sev("Too many switched lights (error at light sw96), max = 96"), Severity::Error);
        assert_eq!(sev("Overlay touching too many faces (touching 70, max 64)"), Severity::Error);
        assert_eq!(sev("Too Many Overlays!"), Severity::Error);
        assert_eq!(sev("Too many brush models in map, max = 4096"), Severity::Error);
        assert_eq!(
            sev("Warning: brushsides uses 91.6% of the engine limit (MAX_MAP_BRUSHSIDES = 65536)"),
            Severity::Warning
        );
        assert_eq!(
            sev("visdata: 200.0% of the engine limit, over it (MAX_MAP_VISIBILITY = 16777216)"),
            Severity::Error
        );
        assert_eq!(sev("#x: warning, not a list item"), Severity::Warning);
        assert_eq!(sev("Bad surface extents - surface is too big to have a lightmap"), Severity::Error);
        assert_eq!(sev("Could not open instance file instances/a.vmf"), Severity::Error);
        assert_eq!(
            sev(
                "Error! prop_static using model \"m.mdl\", which must be used on a dynamic entity (i.e. prop_physics). Deleted."
            ),
            Severity::Error
        );
        assert_eq!(sev("Warning: VBSP: Unknown option \"-foo\" (ignored)"), Severity::Warning);
        assert_eq!(sev("Warning: -glview is not supported by the Hammer Pro compiler (ignored)"), Severity::Warning);
        assert_eq!(sev("Brush 8 (side 2): mixed face contents"), Severity::Warning);
        let l = loc("Brush 8 (side 2): mixed face contents");
        assert_eq!((l.brush_id, l.side_id), (Some(8), Some(2)));
    }

    #[test]
    fn entity_ids() {
        let line = "Entity 17: func_areaportal can only be a single brush";
        assert_eq!(sev(line), Severity::Error);
        assert_eq!(loc(line).entity_id, Some(17));

        let line = "Error: displacement found on a(n) func_detail entity - not supported (entity 3, brush 41)";
        assert_eq!(sev(line), Severity::Error);
        let l = loc(line);
        assert_eq!((l.entity_id, l.brush_id), (Some(3), Some(41)));

        let line = "WARNING: areaportal entity 12 (brush 99) touches > 2 areas";
        assert_eq!(sev(line), Severity::Warning);
        let l = loc(line);
        assert_eq!((l.entity_id, l.brush_id), (Some(12), Some(99)));

        let line = "Error! Entity sky_camera in solid volume! at 128.0 -64.5 32.0";
        assert_eq!(sev(line), Severity::Error);
        let l = loc(line);
        assert_eq!(l.entity_class.as_deref(), Some("sky_camera"));
        assert_eq!(l.position, Some([128.0, -64.5, 32.0]));
    }

    #[test]
    fn positions() {
        let line = "FindPortalSide: Couldn't find a good match for which brush to assign to a portal near (1536.0 2048.0 92.0)";
        assert_eq!(sev(line), Severity::Warning);
        assert_eq!(loc(line).position, Some([1536.0, 2048.0, 92.0]));

        let line = "Static prop models/props_c17/oildrum001.mdl outside the map (100.00, -20.50, 3.00)";
        assert_eq!(sev(line), Severity::Warning);
        assert_eq!(loc(line).position, Some([100.0, -20.5, 3.0]));

        let line = "env_cubemap pointing at deleted brushside near (10, 20, -30)";
        assert_eq!(sev(line), Severity::Warning);
        assert_eq!(loc(line).position, Some([10.0, 20.0, -30.0]));

        let line = "Warning: invalid neighbor connection on displacement near (1.50 2.00 -3.25)";
        assert_eq!(sev(line), Severity::Warning);
        assert_eq!(loc(line).position, Some([1.5, 2.0, -3.25]));

        let line = "Displacement 4 has bad geometry near 10.00 20.00 30.00";
        assert_eq!(sev(line), Severity::Warning);
        assert_eq!(loc(line).position, Some([10.0, 20.0, 30.0]));

        let line = "WARNING: light at (12 -34 56) has missing target";
        assert_eq!(sev(line), Severity::Warning);
        assert_eq!(loc(line).position, Some([12.0, -34.0, 56.0]));

        let line = "MAX_MAP_PORTALVERTS (probably a broken areaportal near 1.0 2.0 3.0";
        assert_eq!(sev(line), Severity::Error);
        assert_eq!(loc(line).position, Some([1.0, 2.0, 3.0]));
    }

    #[test]
    fn errors_and_warnings() {
        for line in [
            "Error loading studio model \"models/props/missing.mdl\"!",
            "LoadPortals: couldn't read c:\\maps\\test.prt",
            "Error opening c:\\maps\\nonexist.bsp",
            "ERROR: MAX_MAP_BRUSHES",
            "Too many brushes in one leaf, max = 32",
            "Too many unique verts, max = 65536 (map has too much brush geometry)",
            "Could not find lights.rad in c:\\maps.",
            "Brush 77: areaportal brush doesn't touch two areas",
            "FATAL ERROR: UtlMemory_CalcNewAllocationCount got to negative values!",
            "The map overflows the max portal count (70000 of max 65536)!",
            "Areaportal leak ! File: c:\\maps\\a.lin ",
        ] {
            assert_eq!(sev(line), Severity::Error, "{line:?}");
        }
        for line in [
            "WARNING: material not found: \"dev/missing\"",
            "Warning: Couldn't open texlight file lights_custom.rad.",
            "!!! WARNING: Can't build collision data!",
            "Can't find surfaceprop metal_box for material metal/a, using default",
            "Candidate brush IDs: 431 432",
            "*** Suppressing further FindPortalSide errors.... ***",
            "WARNING! model \"models/a.mdl\" has mismatching model version (44, expected 48)! Expect problems.",
        ] {
            let s = sev(line);
            assert!(matches!(s, Severity::Warning | Severity::Error), "{line:?}");
        }
        assert_eq!(sev("WARNING: material not found: \"dev/missing\""), Severity::Warning);
        assert_eq!(
            sev(
                "*** Error: Skybox vtf files for skybox/sky_day01_01 weren't compiled with the same size texture and/or same flags!"
            ),
            Severity::Warning
        );
        assert_eq!(sev("Can't load skybox file skybox/sky_day01_01 to build the default cubemap!"), Severity::Warning);
        assert_eq!(loc("Candidate brush IDs: 431 432").brush_id, Some(431));
    }

    #[test]
    fn exit_codes() {
        assert!(describe_exit_code(-1_073_741_819).is_some());
        assert!(describe_exit_code(i32::from_ne_bytes(0xC000_0135u32.to_ne_bytes())).is_some());
        assert_eq!(describe_exit_code(1), None);
        assert_eq!(describe_exit_code(0), None);
    }

    #[test]
    fn triple_parser_rejects_noise() {
        assert_eq!(parse_triple("1 2"), None);
        assert_eq!(parse_triple("1 2 3/4"), None);
        assert_eq!(parse_triple("a b c"), None);
        assert_eq!(parse_triple("1e400 0 0"), None);
        assert_eq!(parse_triple("1,2,3)"), Some([1.0, 2.0, 3.0]));
    }
}
