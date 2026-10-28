use hp_app::settings::ReleaseMode;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Place {
    Pakfile,
    Addon,
}

pub(crate) fn place(vpath: &str, mode: ReleaseMode) -> Option<Place> {
    let gma = hp_gma::whitelist::is_allowed(vpath);
    let bsp = hp_gma::whitelist::bsp_allowed(vpath);
    match mode {
        ReleaseMode::SelfContained | ReleaseMode::ServerKit => bsp.then_some(Place::Pakfile),
        ReleaseMode::WorkshopAddon if gma => Some(Place::Addon),
        ReleaseMode::WorkshopAddon => bsp.then_some(Place::Pakfile),
    }
}

pub(crate) fn skipped(vpath: &str) -> bool {
    hp_gma::whitelist::is_skipped(vpath)
}

pub(crate) fn name_problem(vpath: &str) -> Option<&'static str> {
    if !vpath.is_ascii() {
        return Some("release.name_not_ascii");
    }
    if hp_bsp::normalize_entry_name(vpath).is_err() {
        return Some("release.name_invalid");
    }
    None
}

pub(crate) fn map_name_ok(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

pub(crate) fn addon_folder_name(title: &str, map: &str) -> String {
    let mut out: String = title
        .trim()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c.to_ascii_lowercase() } else { '_' })
        .collect();
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    let out = out.trim_matches('_').to_owned();
    let out = if out.is_empty() { map.to_owned() } else { out };
    const DEVICES: &[&str] = &["con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "lpt1", "lpt2", "lpt3"];
    if DEVICES.contains(&out.as_str()) { format!("{out}_addon") } else { out.chars().take(64).collect() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placement_follows_the_whitelist() {
        let m = ReleaseMode::WorkshopAddon;
        assert_eq!(place("materials/a/b.vmt", m), Some(Place::Addon));
        assert_eq!(place("maps/gm_x_particles.txt", m), Some(Place::Pakfile));
        assert_eq!(place("scripts/soundscapes_gm_x.txt", m), Some(Place::Pakfile));
        assert_eq!(place("scripts/game_sounds_x.txt", m), None);
        assert_eq!(place("scenes/x.vcd", m), Some(Place::Addon));
        let b = ReleaseMode::SelfContained;
        assert_eq!(place("materials/a/b.vmt", b), Some(Place::Pakfile));
        assert_eq!(place("scenes/x.vcd", b), None);
        assert_eq!(place("maps/gm_x_particles.txt", ReleaseMode::ServerKit), Some(Place::Pakfile));
        assert!(skipped("models/a.sw.vtx"));
        assert_eq!(name_problem("materials/é.vmt"), Some("release.name_not_ascii"));
        assert_eq!(name_problem("materials/con.vmt"), Some("release.name_invalid"));
        assert_eq!(name_problem("materials/a.vmt"), None);
    }

    #[test]
    fn names() {
        assert!(map_name_ok("gm_test_v2"));
        assert!(!map_name_ok("gm_Test"));
        assert!(!map_name_ok("gm test"));
        assert!(!map_name_ok("gm.test"));
        assert_eq!(addon_folder_name("My Map: Remastered!", "gm_x"), "my_map_remastered");
        assert_eq!(addon_folder_name("  ", "gm_x"), "gm_x");
        assert_eq!(addon_folder_name("CON", "gm_x"), "con_addon");
        assert_eq!(addon_folder_name("../../evil", "gm_x"), "evil");
    }
}
