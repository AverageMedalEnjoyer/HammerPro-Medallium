use std::path::{Path, PathBuf};
use std::sync::Arc;

use egui::{Id, RichText, ScrollArea, Ui};
use hp_app::{AppState, Severity, t, tf, theme};
use hp_kv::Block;

use crate::widgets::{PickerItem, empty_note, list_row, picker_popup, side_button};

const MAX_FILE_BYTES: u64 = 4 << 20;
const MAX_ITEMS: usize = 4096;

pub(crate) const RULES: [&str; 3] = ["playlooping", "playrandom", "playsoundscape"];

pub(crate) fn soundscape_file(map: &Path) -> Option<PathBuf> {
    let stem = map.file_stem()?.to_str()?;
    let safe = !stem.is_empty() && stem.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    safe.then(|| map.with_file_name(format!("soundscapes_{stem}.txt")))
}

pub(crate) fn valid_name(name: &str) -> bool {
    let n = name.trim();
    !n.is_empty() && n.len() <= 128 && !n.contains(['"', '{', '}']) && !n.contains(char::is_whitespace)
}

pub(crate) fn read_file(path: &Path) -> Result<Block, String> {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Block::new("")),
        Err(e) => return Err(e.to_string()),
    };
    if meta.len() > MAX_FILE_BYTES {
        return Err(tf!("entities.ss_too_big", meta.len()));
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut root = hp_kv::parse_bytes(&bytes).map_err(|e| e.to_string())?;
    root.children.truncate(MAX_ITEMS);
    Ok(root)
}

pub(crate) fn write_file(path: &Path, root: &Block) -> std::io::Result<()> {
    let text = hp_kv::write(&without_empty_values(root));
    let dir = path.parent().ok_or_else(|| std::io::Error::other("no folder"))?;
    let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| std::io::Error::other("bad name"))?;
    let tmp = dir.join(format!(".{name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text.as_bytes())?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

fn without_empty_values(block: &Block) -> Block {
    let mut out = Block::new(block.name.clone());
    out.props = block.props.iter().filter(|(_, v)| !v.trim().is_empty()).cloned().collect();
    out.children = block.children.iter().map(without_empty_values).collect();
    out
}

pub(crate) fn mounted_names(ctx: &egui::Context, assets: &dyn hp_app::AssetsApi) -> Option<Arc<Vec<String>>> {
    crate::catalog::soundscape_names(ctx, assets)
}

pub(crate) fn block_names(text: &[u8]) -> Vec<String> {
    hp_kv::parse_bytes(text)
        .map(|root| root.children.iter().take(MAX_ITEMS).map(|b| b.name.trim().to_ascii_lowercase()).collect())
        .unwrap_or_default()
}

#[derive(Default)]
pub(crate) struct SoundscapeEditor {
    file: Option<PathBuf>,
    root: Block,
    dirty: bool,
    load_error: Option<String>,
    selected: Option<usize>,
    new_name: String,
    message: Option<(Severity, String)>,
}

impl SoundscapeEditor {
    fn sync(&mut self, map: Option<&Path>) {
        let file = map.and_then(soundscape_file);
        if file == self.file {
            return;
        }
        *self = SoundscapeEditor { file: file.clone(), ..SoundscapeEditor::default() };
        if let Some(f) = &file {
            match read_file(f) {
                Ok(root) => self.root = root,
                Err(e) => self.load_error = Some(e),
            }
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !state.runtime.entities.show_soundscapes {
            return;
        }
        self.sync(state.doc.path());
        let mut open = true;
        hp_app::chrome::Window::new(t!("entities.ss_title"))
            .id(Id::unique("hp_ui_entities.soundscapes_window"))
            .open(&mut open)
            .default_size([640.0, 480.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| self.ui(ui, state));
        state.runtime.entities.show_soundscapes = open;
    }

    fn ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let Some(file) = self.file.clone() else {
            empty_note(ui, t!("entities.ss_save_map_first"));
            return;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(file.display().to_string()).small().weak());
            if self.dirty {
                ui.label(RichText::new(t!("entities.ss_unsaved")).small().color(theme::warning()));
            }
        });
        ui.label(RichText::new(t!("entities.ss_hint")).small().weak());
        if let Some(e) = &self.load_error {
            ui.colored_label(theme::error(), tf!("entities.ss_read_failed", e));
        }
        ui.horizontal(|ui| {
            let can_save = self.dirty && self.load_error.is_none();
            if ui.add_enabled(can_save, egui::Button::new(t!("common.save"))).clicked() {
                match write_file(&file, &self.root) {
                    Ok(()) => {
                        self.dirty = false;
                        let text = tf!("entities.ss_saved", file.display());
                        state.notify(Severity::Success, text.clone());
                        self.message = Some((Severity::Success, text));
                    }
                    Err(e) => {
                        let text = tf!("entities.ss_save_failed", e);
                        state.notify(Severity::Error, text.clone());
                        self.message = Some((Severity::Error, text));
                    }
                }
            }
            if ui.button(t!("entities.ss_reload")).clicked() {
                match read_file(&file) {
                    Ok(root) => {
                        self.root = root;
                        self.dirty = false;
                        self.load_error = None;
                        self.selected = None;
                    }
                    Err(e) => self.load_error = Some(e),
                }
            }
        });
        if let Some((sev, text)) = &self.message {
            ui.colored_label(sev.color(), text);
        }
        ui.separator();
        let mounted = mounted_names(ui.ctx(), state.assets.as_ref());
        egui::Panel::left(Id::unique("hp_ui_entities.soundscapes_list"))
            .resizable(true)
            .default_size(200.0)
            .show(ui, |ui| self.list_ui(ui, mounted.as_deref().map(Vec::as_slice)));
        egui::CentralPanel::default().show(ui, |ui| self.details_ui(ui, state));
    }

    fn list_ui(&mut self, ui: &mut Ui, mounted: Option<&[String]>) {
        ui.horizontal(|ui| {
            let w = (ui.available_width() - crate::widgets::SIDE_BUTTON_W - 8.0).max(40.0);
            ui.add(
                egui::TextEdit::singleline(&mut self.new_name).hint_text(t!("entities.ss_new_name")).desired_width(w),
            );
            let ok = valid_name(&self.new_name)
                && !self.root.children.iter().any(|b| b.name.eq_ignore_ascii_case(self.new_name.trim()))
                && self.root.children.len() < MAX_ITEMS;
            let add = ui.add_enabled_ui(ok, |ui| side_button(ui, "+", false)).inner;
            if add.on_hover_text(t!("entities.ss_add")).clicked() {
                let mut b = Block::new(self.new_name.trim());
                b.set("dsp", "1");
                self.root.children.push(b);
                self.selected = Some(self.root.children.len() - 1);
                self.new_name.clear();
                self.dirty = true;
            }
        });
        ScrollArea::vertical().id_salt("hp_ui_entities.ss_names").auto_shrink([false, false]).show(ui, |ui| {
            let mut clicked = None;
            for (i, b) in self.root.children.iter().enumerate() {
                let collides = mounted.is_some_and(|m| m.iter().any(|n| n.eq_ignore_ascii_case(b.name.trim())));
                let detail = if collides { t!("entities.ss_collision_short") } else { "" };
                let resp = list_row(ui, &b.name, detail, self.selected == Some(i), false);
                let resp = if collides { resp.on_hover_text(t!("entities.ss_collision")) } else { resp };
                if resp.clicked() {
                    clicked = Some(i);
                }
            }
            if let Some(i) = clicked {
                self.selected = Some(i);
            }
            if self.root.children.is_empty() {
                ui.label(RichText::new(t!("entities.ss_empty")).weak());
            }
        });
    }

    fn details_ui(&mut self, ui: &mut Ui, state: &mut AppState) {
        let Some(i) = self.selected.filter(|i| *i < self.root.children.len()) else {
            empty_note(ui, t!("entities.ss_select"));
            return;
        };
        let files = state.assets.sounds();
        let volume = state.settings.browser.volume();
        let mut changed = false;
        let mut delete_scape = false;
        {
            let scape = &mut self.root.children[i];
            ui.horizontal(|ui| {
                ui.label(RichText::new(&scape.name).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(t!("entities.ss_delete")).clicked() {
                        delete_scape = true;
                    }
                });
            });
            changed |= key_field(ui, scape, "dsp", t!("entities.ss_dsp"));
            ui.horizontal_wrapped(|ui| {
                for rule in RULES {
                    if ui.small_button(format!("+ {rule}")).clicked() && scape.children.len() < MAX_ITEMS {
                        let mut b = Block::new(rule);
                        b.set("volume", "1");
                        if rule != "playsoundscape" {
                            b.set("pitch", "100");
                        }
                        if rule == "playrandom" {
                            b.set("time", "10,20");
                            let mut waves = Block::new("rndwave");
                            waves.push_prop("wave", "");
                            b.push_child(waves);
                        } else if rule == "playlooping" {
                            b.push_prop("wave", "");
                        } else {
                            b.set("name", "");
                        }
                        scape.children.push(b);
                        changed = true;
                    }
                }
            });
            ui.separator();
            let mut remove_rule = None;
            ScrollArea::vertical().id_salt("hp_ui_entities.ss_rules").auto_shrink([false, false]).show(ui, |ui| {
                for (r, rule) in scape.children.iter_mut().enumerate() {
                    ui.push_id(r, |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&rule.name).strong());
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if side_button(ui, "×", false)
                                        .on_hover_text(t!("entities.ss_remove_rule"))
                                        .clicked()
                                    {
                                        remove_rule = Some(r);
                                    }
                                });
                            });
                            changed |= rule_ui(ui, rule, &files, state.assets.as_ref(), volume);
                        });
                    });
                }
            });
            if let Some(r) = remove_rule {
                scape.children.remove(r);
                changed = true;
            }
        }
        if delete_scape {
            self.root.children.remove(i);
            self.selected = None;
            changed = true;
        }
        if changed {
            self.dirty = true;
        }
    }
}

fn key_field(ui: &mut Ui, block: &mut Block, key: &str, label: &str) -> bool {
    let mut value = block.get(key).unwrap_or("").to_owned();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized([110.0, ui.spacing().interact_size.y], egui::Label::new(label).truncate());
        let resp = ui.add(egui::TextEdit::singleline(&mut value).desired_width(f32::INFINITY));
        if resp.changed() && !value.contains('"') {
            if value.trim().is_empty() {
                block.remove(key);
            } else {
                block.set(key, value.trim());
            }
            changed = true;
        }
    });
    changed
}

fn rule_ui(
    ui: &mut Ui,
    rule: &mut Block,
    files: &[hp_fs::AssetEntry],
    assets: &dyn hp_app::AssetsApi,
    volume: f32,
) -> bool {
    let mut changed = false;
    changed |= key_field(ui, rule, "volume", t!("entities.ss_volume"));
    match rule.name.to_ascii_lowercase().as_str() {
        "playsoundscape" => {
            changed |= key_field(ui, rule, "name", t!("entities.ss_name"));
        }
        kind => {
            changed |= key_field(ui, rule, "pitch", t!("entities.ss_pitch"));
            if kind == "playrandom" {
                changed |= key_field(ui, rule, "time", t!("entities.ss_time"));
                match rule.child_mut("rndwave") {
                    Some(waves) => changed |= waves_ui(ui, waves, files, assets, volume),
                    None => {
                        if ui.small_button(t!("entities.ss_add_wave")).clicked() {
                            let mut waves = Block::new("rndwave");
                            waves.push_prop("wave", "");
                            rule.push_child(waves);
                            changed = true;
                        }
                    }
                }
            } else {
                changed |= wave_row(ui, rule, 0, files, assets, volume);
                changed |= key_field(ui, rule, "position", t!("entities.ss_position"));
            }
            changed |= key_field(ui, rule, "soundlevel", t!("entities.ss_soundlevel"));
        }
    }
    changed
}

fn waves_ui(
    ui: &mut Ui,
    waves: &mut Block,
    files: &[hp_fs::AssetEntry],
    assets: &dyn hp_app::AssetsApi,
    volume: f32,
) -> bool {
    let mut changed = false;
    let count = waves.props.iter().filter(|(k, _)| k.eq_ignore_ascii_case("wave")).count();
    for n in 0..count {
        changed |= wave_row(ui, waves, n, files, assets, volume);
    }
    if ui.small_button(t!("entities.ss_add_wave")).clicked() && count < MAX_ITEMS {
        waves.push_prop("wave", "");
        changed = true;
    }
    changed
}

fn wave_row(
    ui: &mut Ui,
    block: &mut Block,
    n: usize,
    files: &[hp_fs::AssetEntry],
    assets: &dyn hp_app::AssetsApi,
    volume: f32,
) -> bool {
    let Some(slot) =
        block.props.iter().enumerate().filter(|(_, (k, _))| k.eq_ignore_ascii_case("wave")).nth(n).map(|(i, _)| i)
    else {
        if n == 0 && ui.small_button(t!("entities.ss_add_wave")).clicked() {
            block.push_prop("wave", "");
            return true;
        }
        return false;
    };
    let mut changed = false;
    let mut remove = false;
    ui.push_id(("wave", n), |ui| {
        ui.horizontal(|ui| {
            ui.add_sized([110.0, ui.spacing().interact_size.y], egui::Label::new(t!("entities.ss_wave")));
            let w = (ui.available_width() - 3.0 * (crate::widgets::SIDE_BUTTON_W + 8.0)).max(60.0);
            let value = &mut block.props[slot].1;
            if ui.add(egui::TextEdit::singleline(value).desired_width(w)).changed() {
                *value = value.replace('"', "");
                changed = true;
            }
            let btn = side_button(ui, "♪", false).on_hover_text(t!("entities.browse_sounds"));
            if crate::smartedit::picker_is_open(&btn) {
                let picked = picker_popup(&btn, true, files.len(), |i| PickerItem {
                    name: files.get(i).map_or("", |f| crate::values::sound_key_name(&f.name)),
                    detail: "",
                });
                if let Some(p) = picked {
                    block.props[slot].1 = p;
                    changed = true;
                }
            }
            let wave = block.props[slot].1.clone();
            if side_button(ui, "▶", false).on_hover_text(t!("entities.play_sound")).clicked() {
                crate::sounds::play(assets, &wave, &[], volume);
            }
            if side_button(ui, "×", false).on_hover_text(t!("entities.ss_remove_wave")).clicked() {
                remove = true;
            }
        });
    });
    if remove {
        block.props.remove(slot);
        changed = true;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_and_validation() {
        let f = soundscape_file(Path::new("C:/maps/gm_test.vmf")).unwrap();
        assert!(f.ends_with("soundscapes_gm_test.txt"));
        assert_eq!(soundscape_file(Path::new("C:/maps/bad name.vmf")), None);
        assert!(valid_name("gm_test.outside"));
        assert!(!valid_name("a b"));
        assert!(!valid_name("a\"b"));
        assert!(!valid_name(" "));
    }

    #[test]
    fn round_trip_keeps_unknown_keys_and_writes_atomically() {
        let dir = std::env::temp_dir().join(format!("hp-ui-entities-ss-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("soundscapes_x.txt");
        assert_eq!(read_file(&path).map(|b| b.children.len()), Ok(0), "a missing file is empty");
        let text = "\"x.outside\"\n{\n\t\"dsp\" \"1\"\n\t\"custom\" \"kept\"\n\t\"playlooping\"\n\t{\n\t\t\"volume\" \"0.5\"\n\t\t\"wave\" \"ambient/wind/wind1.wav\"\n\t}\n}\n";
        std::fs::write(&path, text).unwrap();
        let root = read_file(&path).unwrap();
        assert_eq!(root.children[0].get("custom"), Some("kept"));
        write_file(&path, &root).unwrap();
        let again = read_file(&path).unwrap();
        assert_eq!(again.children[0].get("custom"), Some("kept"));
        assert_eq!(again.children[0].child("playlooping").and_then(|p| p.get("wave")), Some("ambient/wind/wind1.wav"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary file left");
        let mut with_empty = again.clone();
        with_empty.children[0].push_prop("wave", "  ");
        write_file(&path, &with_empty).unwrap();
        assert!(!std::fs::read_to_string(&path).unwrap().contains("\"  \""));
        assert_eq!(block_names(text.as_bytes()), ["x.outside"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn editor_draws_with_and_without_a_map() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        state.runtime.entities.show_soundscapes = true;
        let mut ed = SoundscapeEditor::default();
        hp_app::testing::run_ctx(&mut state, |ctx, s| ed.show(ctx, s));
        let dir = std::env::temp_dir().join(format!("hp-ui-entities-ss2-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        ed.sync(Some(&dir.join("gm_x.vmf")));
        ed.root.children.push(Block::new("gm_x.a"));
        let mut r = Block::new("playrandom");
        r.push_child(Block::new("rndwave"));
        ed.root.children[0].children.push(r);
        ed.selected = Some(0);
        hp_app::testing::run_ui(&mut state, |ui, s| ed.details_ui(ui, s));
        assert!(!dir.join("soundscapes_gm_x.txt").exists(), "nothing is written without Save");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
