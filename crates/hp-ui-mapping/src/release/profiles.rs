#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Profile {
    pub prefix: &'static str,
    pub name: &'static str,
    pub required: &'static [&'static [&'static str]],
    pub needs_nav: bool,
}

const SPAWNS: &[&str] = &["info_player_start", "info_player_deathmatch", "info_player_combine", "info_player_rebel"];

pub(crate) const PROFILES: &[Profile] = &[
    Profile { prefix: "gm_", name: "Sandbox", required: &[SPAWNS], needs_nav: false },
    Profile {
        prefix: "ttt_",
        name: "Trouble in Terrorist Town",
        required: &[&["info_player_*"], &["weapon_*", "item_*", "ttt_random_weapon", "ttt_random_ammo"]],
        needs_nav: false,
    },
    Profile {
        prefix: "ph_",
        name: "Prop Hunt",
        required: &[&["info_player_terrorist"], &["info_player_counterterrorist"]],
        needs_nav: false,
    },
    Profile { prefix: "rp_", name: "DarkRP", required: &[SPAWNS], needs_nav: false },
    Profile { prefix: "zs_", name: "Zombie Survival", required: &[&["info_player_*"]], needs_nav: true },
    Profile { prefix: "mu_", name: "Murder", required: &[&["info_player_*"]], needs_nav: false },
    Profile {
        prefix: "deathrun_",
        name: "Deathrun",
        required: &[&["info_player_terrorist"], &["info_player_counterterrorist"]],
        needs_nav: false,
    },
    Profile {
        prefix: "jb_",
        name: "Jailbreak",
        required: &[&["info_player_terrorist"], &["info_player_counterterrorist"]],
        needs_nav: false,
    },
    Profile { prefix: "bhop_", name: "Bunny Hop", required: &[&["info_player_*"]], needs_nav: false },
    Profile { prefix: "surf_", name: "Surf", required: &[&["info_player_*"]], needs_nav: false },
];

pub(crate) fn profile_for(map: &str) -> Option<&'static Profile> {
    let lower = map.to_ascii_lowercase();
    PROFILES.iter().filter(|p| lower.starts_with(p.prefix)).max_by_key(|p| p.prefix.len())
}

fn matches(pattern: &str, class: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => class.starts_with(prefix),
        None => class == pattern,
    }
}

pub(crate) fn missing_groups<'a>(profile: &Profile, classes: impl Iterator<Item = &'a str> + Clone) -> Vec<String> {
    profile
        .required
        .iter()
        .filter(|group| !classes.clone().any(|c| group.iter().any(|p| matches(p, &c.to_ascii_lowercase()))))
        .map(|group| group.join(" / "))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_check_classes() {
        assert_eq!(profile_for("TTT_Minecraft").map(|p| p.prefix), Some("ttt_"));
        assert_eq!(profile_for("de_dust"), None);
        let ttt = profile_for("ttt_x").unwrap();
        let classes = ["info_player_start", "prop_physics"];
        assert_eq!(
            missing_groups(ttt, classes.iter().copied()),
            vec!["weapon_* / item_* / ttt_random_weapon / ttt_random_ammo".to_owned()]
        );
        let classes = ["info_player_start", "weapon_zm_shotgun"];
        assert!(missing_groups(ttt, classes.iter().copied()).is_empty());
        let ph = profile_for("ph_office").unwrap();
        assert_eq!(missing_groups(ph, ["info_player_terrorist"].iter().copied()).len(), 1);
    }
}
