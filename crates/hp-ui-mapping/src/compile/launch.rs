use egui::{RichText, Ui};
use hp_app::runtime::GameState;
use hp_app::settings::{LaunchTiming, Settings};
use hp_app::{AppState, t};
use hp_compile::launch::{
    ArgSpec, CONSOLE, DEV, HEIGHT, MAXPLAYERS, NOBORDER, NOVID, SV_LAN, WIDTH, WINDOWED, arg_get, arg_set,
};

pub(crate) fn launches_itself(settings: &Settings) -> bool {
    settings.compile.launch_game
        && !settings.compile.launch_via_steam
        && settings.launch.timing == LaunchTiming::AtStart
        && settings.game.is_gmod()
}

pub(crate) fn launch_flags(state: &AppState, entities_only: bool) -> Vec<&'static str> {
    let used = if state.settings.launch.skip_unused_content { hp_app::runtime::content_use(state) } else { None };
    launch_flags_with(state, used, entities_only)
}

pub(crate) fn launch_flags_with(
    state: &AppState,
    used: Option<hp_app::runtime::ContentUse>,
    entities_only: bool,
) -> Vec<&'static str> {
    let mut out = Vec::new();
    if state.settings.launch.skip_unused_content
        && let Some(used) = used
    {
        out.extend(hp_compile::launch::content_skip_args(used.workshop, used.addons));
    }
    if entities_only {
        out.push(hp_compile::launch::STALE_ZIP_FLAG);
    }
    out
}

const DEFAULT_SIZE: (u32, u32) = (1600, 900);

pub(crate) fn options_ui(ui: &mut Ui, state: &mut AppState, enabled: bool) {
    let launch_on = state.settings.compile.launch_game;
    let args_used = launch_on && !state.settings.compile.launch_via_steam;
    ui.add_space(4.0);
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(t!("launch.title")).strong());
            let button = ui
                .add_enabled(args_used, egui::Button::new(format!("{}  ⏷", t!("launch.fast_start"))))
                .on_hover_text(t!("launch.fast_start_tip"));
            egui::Popup::from_toggle_button_response(&button)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| fast_start_ui(ui, state));
            ui.separator();
            ui.add_enabled_ui(launch_on, |ui| timing_ui(ui, state));
            ui.separator();
            game_state_ui(ui, state.runtime.launch.game);
        });
    });
}

fn timing_ui(ui: &mut Ui, state: &mut AppState) {
    let s = &mut state.settings.launch;
    let before = (s.timing, s.reuse_running);
    ui.label(t!("launch.timing"));
    ui.selectable_value(&mut s.timing, LaunchTiming::AtEnd, t!("launch.at_end")).on_hover_text(t!("launch.at_end_tip"));
    ui.selectable_value(&mut s.timing, LaunchTiming::AtStart, t!("launch.at_start"))
        .on_hover_text(t!("launch.at_start_tip"));
    ui.checkbox(&mut s.reuse_running, t!("launch.reuse")).on_hover_text(t!("launch.reuse_tip"));
    if (s.timing, s.reuse_running) != before {
        state.settings_dirty = true;
    }
}

fn game_state_ui(ui: &mut Ui, game: GameState) {
    let (text, tip, color) = match game {
        GameState::Ready => ("launch.game_ready", "launch.game_ready_tip", hp_app::theme::accent()),
        GameState::Starting => ("launch.game_starting", "launch.game_starting_tip", ui.visuals().text_color()),
        GameState::NotRunning | GameState::Unknown => {
            ("launch.game_off", "launch.game_off_tip", ui.visuals().weak_text_color())
        }
    };
    ui.label(RichText::new(format!("⏺ {}", t!(text))).color(color)).on_hover_text(t!(tip));
}

fn flag_checkbox(ui: &mut Ui, args: &mut String, spec: ArgSpec, label: &str, tip: &str) -> bool {
    let mut on = arg_get(args, spec).is_some();
    let changed = ui.checkbox(&mut on, label).on_hover_text(format!("{tip} ({})", spec.name)).changed();
    if changed {
        *args = arg_set(args, spec, on.then_some(&[]));
    }
    changed
}

fn number_of(args: &str, spec: ArgSpec) -> Option<u32> {
    arg_get(args, spec)?.first()?.parse().ok()
}

fn fast_start_ui(ui: &mut Ui, state: &mut AppState) {
    ui.set_min_width(340.0);
    ui.label(RichText::new(t!("launch.fast_start_note")).weak().small());
    ui.add_space(4.0);
    let args = &mut state.settings.compile.game_args;
    let mut changed = false;
    changed |= flag_checkbox(ui, args, NOVID, t!("launch.novid"), t!("launch.novid_tip"));
    changed |= flag_checkbox(ui, args, WINDOWED, t!("launch.windowed"), t!("launch.windowed_tip"));
    changed |= flag_checkbox(ui, args, NOBORDER, t!("launch.borderless"), t!("launch.borderless_tip"));

    let (w, h) = (number_of(args, WIDTH), number_of(args, HEIGHT));
    let mut sized = w.is_some() && h.is_some();
    ui.horizontal(|ui| {
        if ui.checkbox(&mut sized, t!("launch.size")).on_hover_text(t!("launch.size_tip")).changed() {
            let (w, h) = (w.unwrap_or(DEFAULT_SIZE.0).to_string(), h.unwrap_or(DEFAULT_SIZE.1).to_string());
            *args = arg_set(args, WIDTH, sized.then_some(&[w.as_str()]));
            *args = arg_set(args, HEIGHT, sized.then_some(&[h.as_str()]));
            changed = true;
        }
        if sized {
            let mut w = w.unwrap_or(DEFAULT_SIZE.0);
            let mut h = h.unwrap_or(DEFAULT_SIZE.1);
            let rw = ui.add(egui::DragValue::new(&mut w).range(320..=7680).speed(8.0));
            ui.label("x");
            let rh = ui.add(egui::DragValue::new(&mut h).range(240..=4320).speed(8.0));
            if rw.changed() || rh.changed() {
                *args = arg_set(args, WIDTH, Some(&[w.to_string().as_str()]));
                *args = arg_set(args, HEIGHT, Some(&[h.to_string().as_str()]));
                changed = true;
            }
        }
    });

    changed |= flag_checkbox(ui, args, CONSOLE, t!("launch.console"), t!("launch.console_tip"));
    changed |= flag_checkbox(ui, args, DEV, t!("launch.dev"), t!("launch.dev_tip"));

    let lan = arg_get(args, SV_LAN).is_some_and(|v| v.first().map(String::as_str) == Some("1"));
    let solo = arg_get(args, MAXPLAYERS).is_some_and(|v| v.first().map(String::as_str) == Some("1"));
    let mut private = lan && solo;
    if ui
        .checkbox(&mut private, t!("launch.private"))
        .on_hover_text(format!("{} (+sv_lan 1 +maxplayers 1)", t!("launch.private_tip")))
        .changed()
    {
        *args = arg_set(args, SV_LAN, private.then_some(&["1"]));
        *args = arg_set(args, MAXPLAYERS, private.then_some(&["1"]));
        changed = true;
    }

    ui.separator();
    let launch = &mut state.settings.launch;
    changed |= ui
        .checkbox(&mut launch.skip_unused_content, t!("launch.skip_content"))
        .on_hover_text(t!("launch.skip_content_tip"))
        .changed();
    changed |= ui
        .checkbox(&mut launch.load_report, t!("launch.load_report"))
        .on_hover_text(t!("launch.load_report_tip"))
        .changed();
    if let Some(last) = &state.runtime.launch.last_load {
        let secs = format!("{:.1}", last.total.as_secs_f64());
        ui.label(RichText::new(hp_app::tf!("launch.last_load", last.map, secs)).weak().small());
    }
    if changed {
        state.settings_dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_read_from_the_arguments() {
        assert_eq!(number_of("-w 1280 -h 720", WIDTH), Some(1280));
        assert_eq!(number_of("-w big", WIDTH), None);
        assert_eq!(number_of("-console", HEIGHT), None);
    }

    #[test]
    fn options_draw_without_changing_the_settings() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.settings.compile.launch_game = true;
        state.settings.compile.game_args = "-console -w 1280 -h 720 +sv_lan 1 +maxplayers 1".to_owned();
        for game in [GameState::Unknown, GameState::Starting, GameState::Ready] {
            state.runtime.launch.game = game;
            hp_app::testing::run_ui(&mut state, |ui, state| {
                options_ui(ui, state, true);
                fast_start_ui(ui, state);
            });
        }
        assert!(!state.settings_dirty, "drawing changes nothing");
        assert_eq!(state.settings.compile.game_args, "-console -w 1280 -h 720 +sv_lan 1 +maxplayers 1");
    }

    #[test]
    fn launch_decisions() {
        let mut state = hp_app::testing::test_state();
        state.settings.compile.launch_game = true;
        assert!(!launches_itself(&state.settings), "after the compile: the plan launches");
        state.settings.launch.timing = LaunchTiming::AtStart;
        assert!(launches_itself(&state.settings));
        state.settings.compile.launch_via_steam = true;
        assert!(!launches_itself(&state.settings), "Steam launches go through the plan");
        assert!(launch_flags(&state, false).is_empty());
        assert_eq!(launch_flags(&state, true), vec!["-allowstalezip"]);
        state.settings.launch.skip_unused_content = true;
        assert!(launch_flags(&state, false).is_empty());
        let used = hp_app::runtime::ContentUse { workshop: false, addons: true };
        assert_eq!(launch_flags_with(&state, Some(used), false), vec!["-noworkshop"]);
        state.settings.launch.skip_unused_content = false;
        assert!(launch_flags_with(&state, Some(used), false).is_empty(), "the option is off");
    }

    #[test]
    fn the_launch_is_left_to_the_binary_when_the_game_boots_during_the_compile() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.settings.game.gmod_root = Some(crate::test_scratch("launch_deferred").join("no_game"));
        let dir = crate::test_scratch("launch_deferred_map");
        state.doc.set_path(Some(dir.join("gm_play.vmf")));
        state.settings.compile.launch_game = true;
        state.settings.launch.timing = LaunchTiming::AtStart;
        let ctx = egui::Context::default();
        let start = |state: &mut AppState| {
            let mut d = super::super::CompileDialog::new();
            d.start(&ctx, state);
            d.cancel(state);
            (state.runtime.launch.compile_launch_deferred, state.runtime.launch.compile_launch_map.clone())
        };
        assert_eq!(start(&mut state), (true, Some("gm_play".to_owned())));
        state.settings.launch.timing = LaunchTiming::AtEnd;
        assert_eq!(start(&mut state), (false, Some("gm_play".to_owned())), "the plan launches");
        state
            .doc
            .add_cordon("room", hp_geom::Aabb::new(glam::DVec3::splat(-64.0), glam::DVec3::splat(64.0)))
            .expect("cordon");
        state.doc.set_cordons_enabled(true).expect("cordons on");
        state.settings.compile.compile_cordon_only = true;
        assert_eq!(start(&mut state), (false, Some("gm_play_cordon".to_owned())));
        state.settings.compile.launch_game = false;
        assert_eq!(start(&mut state), (false, None), "the compile does not run the map");
    }

    #[test]
    fn the_glyphs_exist() {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        let font = egui::FontId::proportional(14.0);
        for glyph in ["⏺", "⏷"] {
            assert!(ctx.fonts_mut(|f| f.has_glyphs(&font, glyph)), "{glyph}");
        }
    }
}
