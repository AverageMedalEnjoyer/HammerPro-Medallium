//! The game configuration picker and the "Configure Hammer" editor with its live text preview.

use std::path::PathBuf;

use egui::{RichText, Ui};
use hp_app::{AppState, Severity, t, tf};
use hp_fs::gameconfig::{ConfigFile, ConfigId, GameConfig, KvBlock as Block, parse_entry, write_entry};

use crate::game_ui::GameJobs;
use crate::jobs::{PickRequest, PickTarget};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigField {
    GameData(usize),
    GameExeDir,
    GameDir,
    MapDir,
    PrefabDir,
    BspDir,
    GameExe,
    Bsp,
    Vis,
    Light,
    Mdl,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum EditorTab {
    #[default]
    Game,
    Build,
}

#[derive(Debug, Default)]
pub struct ConfigEditor {
    loaded: String,
    file: Option<ConfigFile>,
    entry: Option<Block>,
    cfg: GameConfig,
    preview: String,
    error: Option<String>,
    status: Option<(Severity, String)>,
    dirty: bool,
    tab: EditorTab,
}

impl ConfigEditor {
    fn load(&mut self, id: &str) {
        *self = ConfigEditor { tab: self.tab, ..ConfigEditor::default() };
        self.loaded = id.to_owned();
        let Some(cid) = ConfigId::parse(id) else { return };
        match ConfigFile::load(&cid.file) {
            Ok(file) => {
                if let Some(entry) = file.game(&cid.name).cloned() {
                    self.cfg = GameConfig::from_block(&entry);
                    self.preview = write_entry(&entry);
                    self.entry = Some(entry);
                } else {
                    self.error = Some(tf!("shell.gc.missing_entry", cid.name));
                }
                self.file = Some(file);
            }
            Err(e) => self.error = Some(tf!("shell.gc.cannot_read", cid.file.display(), e)),
        }
    }

    fn fields_changed(&mut self) {
        if let Some(entry) = &mut self.entry {
            self.cfg.apply_to(entry);
            self.preview = write_entry(entry);
        }
        self.error = None;
        self.dirty = true;
        self.status = None;
    }

    fn preview_changed(&mut self) {
        match parse_entry(&self.preview) {
            Ok(block) if block.name == self.cfg.name => {
                self.cfg = GameConfig::from_block(&block);
                self.entry = Some(block);
                self.error = None;
                self.dirty = true;
                self.status = None;
            }
            Ok(_) => self.error = Some(t!("shell.gc.name_locked").to_owned()),
            Err(e) => self.error = Some(e),
        }
    }

    fn save(&mut self) -> bool {
        let (Some(file), Some(entry)) = (self.file.as_mut(), self.entry.clone()) else { return false };
        if self.error.is_some() {
            return false;
        }
        file.replace_game(&self.cfg.name, entry);
        match file.save() {
            Ok(()) => {
                self.dirty = false;
                self.status = Some((Severity::Success, tf!("shell.gc.saved", file.path.display())));
                true
            }
            Err(e) => {
                self.status = Some((Severity::Error, tf!("shell.gc.save_failed", file.path.display(), e)));
                false
            }
        }
    }

    fn slot(&mut self, field: ConfigField) -> Option<&mut String> {
        let c = &mut self.cfg;
        Some(match field {
            ConfigField::GameData(i) => c.game_data.get_mut(i)?,
            ConfigField::GameExeDir => &mut c.game_exe_dir,
            ConfigField::GameDir => &mut c.game_dir,
            ConfigField::MapDir => &mut c.map_dir,
            ConfigField::PrefabDir => &mut c.prefab_dir,
            ConfigField::BspDir => &mut c.bsp_dir,
            ConfigField::GameExe => &mut c.game_exe,
            ConfigField::Bsp => &mut c.bsp,
            ConfigField::Vis => &mut c.vis,
            ConfigField::Light => &mut c.light,
            ConfigField::Mdl => &mut c.mdl,
        })
    }
}

fn pick_request(field: ConfigField, start: Option<PathBuf>) -> PickRequest {
    let (target, title, filter) = match field {
        ConfigField::GameData(_) => (
            PickTarget::ConfigFile,
            t!("shell.gc.pick_fgd"),
            Some((t!("shell.game.fgd_filter").to_owned(), vec!["fgd"])),
        ),
        ConfigField::GameExe | ConfigField::Bsp | ConfigField::Vis | ConfigField::Light | ConfigField::Mdl => {
            (PickTarget::ConfigFile, t!("shell.gc.pick_exe"), Some((t!("shell.gc.exe_filter").to_owned(), vec!["exe"])))
        }
        _ => (PickTarget::ConfigFolder, t!("shell.gc.pick_folder"), None),
    };
    PickRequest { target, title: title.to_owned(), start_dir: start, filter }
}

fn field_row(
    ui: &mut Ui,
    ed: &mut ConfigEditor,
    jobs: &mut GameJobs,
    label: &str,
    field: ConfigField,
    browse: bool,
) -> bool {
    let mut changed = false;
    let ctx = ui.ctx().clone();
    ui.label(RichText::new(label).small());
    ui.horizontal(|ui| {
        let width = (ui.available_width() - if browse { 84.0 } else { 8.0 }).max(120.0);
        if let Some(text) = ed.slot(field) {
            changed |= ui.add(egui::TextEdit::singleline(text).desired_width(width)).changed();
        }
        if browse && ui.add_enabled(!jobs.picking(), egui::Button::new(t!("common.browse"))).clicked() {
            let start = ed.slot(field).map(|s| PathBuf::from(s.clone())).and_then(|p| {
                if p.is_dir() { Some(p) } else { p.parent().map(PathBuf::from) }
            });
            jobs.config_field = Some(field);
            jobs.pick(&ctx, pick_request(field, start));
        }
    });
    changed
}

fn plain_row(ui: &mut Ui, label: &str, text: &mut String, width: f32) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).small());
        changed = ui.add(egui::TextEdit::singleline(text).desired_width(width)).changed();
    });
    changed
}

/// Every configuration file found on the computer, with the folder it comes from.
pub fn config_list(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    let ctx = ui.ctx().clone();
    let list = hp_app::games::configs(Some(&ctx));
    ui.horizontal(|ui| {
        if list.scanning {
            ui.spinner();
            ui.label(tf!("shell.gc.searching", list.files));
        } else {
            ui.label(tf!("shell.gc.found", list.entries.len(), list.files));
        }
        if ui
            .add_enabled(!list.scanning, egui::Button::new(t!("shell.gc.rescan")))
            .on_hover_text(t!("shell.gc.rescan_tip"))
            .clicked()
        {
            hp_app::games::rescan(Some(&ctx));
        }
        if ui.add_enabled(!jobs.picking(), egui::Button::new(t!("shell.gc.add_file"))).clicked() {
            jobs.config_field = None;
            jobs.pick(
                &ctx,
                PickRequest {
                    target: PickTarget::ConfigAddFile,
                    title: t!("shell.gc.pick_config").to_owned(),
                    start_dir: None,
                    filter: Some((t!("shell.gc.config_filter").to_owned(), vec!["txt"])),
                },
            );
        }
    });
    if list.entries.is_empty() {
        ui.label(RichText::new(t!("shell.gc.none")).weak().small());
        return;
    }
    let current = state.settings.game.game.clone();
    let mut chosen = None;
    egui::ScrollArea::vertical().id_salt("hp_gc_list").max_height(190.0).auto_shrink([false, true]).show(ui, |ui| {
        for e in &list.entries {
            let selected = e.key() == current;
            ui.horizontal(|ui| {
                let r = ui.add(egui::Button::selectable(selected, RichText::new(e.label()).strong()));
                ui.label(RichText::new(e.dir.display().to_string()).weak().small().monospace());
                if r.clicked() && !selected {
                    chosen = Some(e.key());
                }
            });
        }
    });
    if let Some(id) = chosen {
        hp_app::games::choose_game(state, &id, false);
        state.settings_dirty = true;
    }
}

/// The classic "Configure Hammer" pages for the selected configuration, with an editable preview.
pub fn configure_hammer(ui: &mut Ui, state: &mut AppState, jobs: &mut GameJobs) {
    let id = state.settings.game.game.trim().to_owned();
    let mut ed = std::mem::take(&mut jobs.editor);
    if ed.loaded != id {
        ed.load(&id);
    }
    if let Some((field, path)) = jobs.config_pick.take()
        && let Some(slot) = ed.slot(field)
    {
        *slot = path.display().to_string();
        ed.fields_changed();
    }
    if ConfigId::parse(&id).is_none() {
        ui.label(RichText::new(t!("shell.gc.choose_first")).weak());
        jobs.editor = ed;
        return;
    }
    if ed.entry.is_none() {
        let msg = ed.error.clone().unwrap_or_default();
        ui.label(RichText::new(msg).color(crate::common::severity_color(ui.visuals(), Severity::Error)));
        jobs.editor = ed;
        return;
    }
    ui.horizontal(|ui| {
        ui.selectable_value(&mut ed.tab, EditorTab::Game, t!("shell.gc.tab_game"));
        ui.selectable_value(&mut ed.tab, EditorTab::Build, t!("shell.gc.tab_build"));
    });
    ui.separator();
    let before = ed.cfg.clone();
    let mut changed = false;
    let name = ed.cfg.name.clone();
    match ed.tab {
        EditorTab::Game => {
            ui.label(RichText::new(format!("{}: {name}", t!("shell.gc.configuration"))).strong());
            ui.label(RichText::new(t!("shell.gc.game_data")).small());
            let mut remove = None;
            let ctx = ui.ctx().clone();
            for i in 0..ed.cfg.game_data.len() {
                ui.horizontal(|ui| {
                    let width = (ui.available_width() - 116.0).max(120.0);
                    changed |= ui.add(egui::TextEdit::singleline(&mut ed.cfg.game_data[i]).desired_width(width)).changed();
                    if ui.add_enabled(!jobs.picking(), egui::Button::new(t!("common.browse"))).clicked() {
                        jobs.config_field = Some(ConfigField::GameData(i));
                        jobs.pick(&ctx, pick_request(ConfigField::GameData(i), None));
                    }
                    if ui.small_button("✖").on_hover_text(t!("common.remove")).clicked() {
                        remove = Some(i);
                    }
                });
            }
            if let Some(i) = remove {
                ed.cfg.game_data.remove(i);
                changed = true;
            }
            if ui.button(format!("➕ {}", t!("shell.gc.add_fgd"))).clicked() {
                ed.cfg.game_data.push(String::new());
                changed = true;
            }
            ui.add_space(4.0);
            changed |= plain_row(ui, t!("shell.gc.point_entity"), &mut ed.cfg.default_point_entity, 220.0);
            changed |= plain_row(ui, t!("shell.gc.solid_entity"), &mut ed.cfg.default_solid_entity, 220.0);
            changed |= plain_row(ui, t!("shell.gc.texture_scale"), &mut ed.cfg.texture_scale, 80.0);
            changed |= plain_row(ui, t!("shell.gc.lightmap_scale"), &mut ed.cfg.lightmap_scale, 80.0);
            changed |= plain_row(ui, t!("shell.gc.cordon"), &mut ed.cfg.cordon_texture, 260.0);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.game_exe_dir"), ConfigField::GameExeDir, true);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.game_dir"), ConfigField::GameDir, true);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.map_dir"), ConfigField::MapDir, true);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.prefab_dir"), ConfigField::PrefabDir, true);
        }
        EditorTab::Build => {
            ui.label(RichText::new(t!("shell.gc.build_note")).weak().small());
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.game_exe"), ConfigField::GameExe, true);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.bsp_exe"), ConfigField::Bsp, true);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.vis_exe"), ConfigField::Vis, true);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.rad_exe"), ConfigField::Light, true);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.bsp_dir"), ConfigField::BspDir, true);
            changed |= field_row(ui, &mut ed, jobs, t!("shell.gc.mdl_exe"), ConfigField::Mdl, true);
        }
    }
    if changed || ed.cfg != before {
        ed.fields_changed();
    }

    ui.add_space(6.0);
    ui.label(RichText::new(t!("shell.gc.preview")).strong());
    let file_text = ed.file.as_ref().map(|f| f.path.display().to_string()).unwrap_or_default();
    ui.label(RichText::new(file_text).weak().small().monospace());
    egui::ScrollArea::vertical().id_salt("hp_gc_preview").max_height(190.0).show(ui, |ui| {
        let r = ui.add(
            egui::TextEdit::multiline(&mut ed.preview)
                .font(egui::TextStyle::Monospace)
                .code_editor()
                .desired_rows(10)
                .desired_width(f32::INFINITY),
        );
        if r.changed() {
            ed.preview_changed();
        }
    });
    if let Some(e) = &ed.error {
        ui.label(RichText::new(e).color(crate::common::severity_color(ui.visuals(), Severity::Warning)));
    }
    if let Some((sev, text)) = &ed.status {
        ui.label(RichText::new(text).color(crate::common::severity_color(ui.visuals(), *sev)));
    }
    ui.horizontal(|ui| {
        let can_save = ed.dirty && ed.error.is_none();
        if ui.add_enabled(can_save, egui::Button::new(t!("shell.gc.apply"))).on_hover_text(t!("shell.gc.apply_tip")).clicked()
            && ed.save()
        {
            hp_app::games::reload_active(state);
        }
        if ui.add_enabled(ed.dirty, egui::Button::new(t!("shell.gc.revert"))).clicked() {
            ed.load(&id);
        }
        if ed.dirty {
            ui.label(RichText::new(t!("shell.gc.unsaved")).weak().small());
        }
    });
    jobs.editor = ed;
}

pub(crate) const TR: hp_app::i18n::TrTable = &[
    ("shell.gc.searching", "Searching this computer... {0} files found so far", "Recherche sur cet ordinateur... {0} fichiers trouvés"),
    ("shell.gc.found", "{0} game configurations in {1} files", "{0} configurations de jeu dans {1} fichiers"),
    ("shell.gc.rescan", "Search again", "Chercher à nouveau"),
    (
        "shell.gc.rescan_tip",
        "Looks for every gameconfig.txt and hammerplusplus_gameconfig.txt on this computer again.",
        "Cherche à nouveau tous les gameconfig.txt et hammerplusplus_gameconfig.txt de cet ordinateur.",
    ),
    ("shell.gc.add_file", "Add a file...", "Ajouter un fichier..."),
    ("shell.gc.pick_config", "Choose a gameconfig.txt or hammerplusplus_gameconfig.txt", "Choisissez un gameconfig.txt ou hammerplusplus_gameconfig.txt"),
    ("shell.gc.config_filter", "Game configuration (*.txt)", "Configuration de jeu (*.txt)"),
    (
        "shell.gc.none",
        "No game configuration found yet. Use Add a file... to pick one by hand.",
        "Aucune configuration de jeu trouvée pour l'instant. Utilisez Ajouter un fichier... pour en choisir une.",
    ),
    (
        "shell.gc.choose_first",
        "Choose a game configuration above to edit it.",
        "Choisissez une configuration de jeu ci-dessus pour la modifier.",
    ),
    ("shell.gc.tab_game", "Game Configurations", "Configurations de jeu"),
    ("shell.gc.tab_build", "Build Programs", "Programmes de compilation"),
    ("shell.gc.configuration", "Configuration", "Configuration"),
    ("shell.gc.game_data", "Game Data files (.fgd):", "Fichiers Game Data (.fgd) :"),
    ("shell.gc.add_fgd", "Add a Game Data file", "Ajouter un fichier Game Data"),
    ("shell.gc.point_entity", "Default PointEntity class:", "Classe PointEntity par défaut :"),
    ("shell.gc.solid_entity", "Default SolidEntity class:", "Classe SolidEntity par défaut :"),
    ("shell.gc.texture_scale", "Default texture scale:", "Échelle de texture par défaut :"),
    ("shell.gc.lightmap_scale", "Default lightmap scale:", "Échelle de lightmap par défaut :"),
    ("shell.gc.cordon", "Cordon texture:", "Texture du cordon :"),
    ("shell.gc.game_exe_dir", "Game Executable Directory (ex: c:\\Half-Life):", "Dossier de l'exécutable du jeu (ex : c:\\Half-Life) :"),
    ("shell.gc.game_dir", "Game Directory (where gameinfo.txt is, ex: c:\\Half-Life\\cstrike):", "Dossier du jeu (où se trouve gameinfo.txt, ex : c:\\Half-Life\\cstrike) :"),
    ("shell.gc.map_dir", "Hammer VMF Directory (ex: C:\\Half-Life\\maps):", "Dossier des VMF de Hammer (ex : C:\\Half-Life\\maps) :"),
    ("shell.gc.prefab_dir", "Prefab Directory (ex: C:\\Half-Life\\maps\\prefabs):", "Dossier des prefabs (ex : C:\\Half-Life\\maps\\prefabs) :"),
    (
        "shell.gc.build_note",
        "The Compile button runs these programs. Select the build and game executables for this game configuration.",
        "Le bouton Compiler lance ces programmes. Choisissez les exécutables de compilation et de jeu de cette configuration.",
    ),
    ("shell.gc.game_exe", "Game executable:", "Exécutable du jeu :"),
    ("shell.gc.bsp_exe", "BSP executable (vbsp):", "Exécutable BSP (vbsp) :"),
    ("shell.gc.vis_exe", "VIS executable (vvis):", "Exécutable VIS (vvis) :"),
    ("shell.gc.rad_exe", "RAD executable (vrad):", "Exécutable RAD (vrad) :"),
    ("shell.gc.bsp_dir", "Place compiled maps in this directory before running the game:", "Placer les maps compilées dans ce dossier avant de lancer le jeu :"),
    ("shell.gc.mdl_exe", "MDL executable (studiomdl):", "Exécutable MDL (studiomdl) :"),
    ("shell.gc.preview", "Preview of this configuration (editable):", "Aperçu de cette configuration (modifiable) :"),
    ("shell.gc.apply", "Apply", "Appliquer"),
    (
        "shell.gc.apply_tip",
        "Writes the configuration to its file and loads it.",
        "Écrit la configuration dans son fichier et la charge.",
    ),
    ("shell.gc.revert", "Revert", "Annuler"),
    ("shell.gc.unsaved", "Not applied yet", "Pas encore appliqué"),
    ("shell.gc.saved", "Saved {0}", "Enregistré : {0}"),
    ("shell.gc.save_failed", "Cannot write {0}: {1}", "Impossible d'écrire {0} : {1}"),
    ("shell.gc.cannot_read", "Cannot read {0}: {1}", "Impossible de lire {0} : {1}"),
    ("shell.gc.missing_entry", "{0} is not in its file any more.", "{0} n'est plus dans son fichier."),
    (
        "shell.gc.name_locked",
        "The name of a configuration cannot be changed here.",
        "Le nom d'une configuration ne peut pas être changé ici.",
    ),
    ("shell.gc.pick_fgd", "Choose a Game Data file", "Choisissez un fichier Game Data"),
    ("shell.gc.pick_exe", "Choose a program", "Choisissez un programme"),
    ("shell.gc.exe_filter", "Programs (*.exe)", "Programmes (*.exe)"),
    ("shell.gc.pick_folder", "Choose a folder", "Choisissez un dossier"),
    ("shell.gc.list_heading", "Game configurations on this computer", "Configurations de jeu de cet ordinateur"),
    ("shell.gc.editor_heading", "Configure Hammer", "Configurer Hammer"),
];
