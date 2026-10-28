use egui::{Id, RichText, ScrollArea, Ui};
use hp_app::{AppState, t, tf};
use hp_fgd::{IoDef, ResolvedClass, ResolvedFgd};

use crate::names::WordSearch;

pub(crate) fn vdc_url(class: &str) -> String {
    let safe: String = class.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
    format!("https://developer.valvesoftware.com/wiki/{safe}")
}

const NAME_COLUMN_W: f32 = 150.0;
const TYPE_COLUMN_W: f32 = 90.0;

#[derive(Default)]
pub(crate) struct HelpWindow {
    filter: String,
}

impl HelpWindow {
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, fgd: Option<&ResolvedFgd>, focus_before: bool) {
        let Some(class) = state.runtime.entities.help_class.clone() else { return };
        let mut open = true;
        let fgd = fgd.or_else(|| state.assets.fgd());
        let resp = hp_app::chrome::Window::new(tf!("entities.help_title", class))
            .id(Id::unique("hp_ui_entities.help_window"))
            .open(&mut open)
            .default_size([520.0, 560.0])
            .resizable(true)
            .collapsible(true)
            .show(ctx, |ui| match fgd.and_then(|f| f.class(&class)) {
                Some(c) => self.page(ui, c),
                None => {
                    ui.label(RichText::new(t!("entities.help_unknown")).weak());
                    ui.hyperlink_to(t!("entities.help_vdc"), vdc_url(&class));
                }
            });
        if crate::dialogs::escape_closes(ctx, resp.as_ref().map(|r| r.response.layer_id), focus_before) {
            open = false;
        }
        if !open {
            state.runtime.entities.help_class = None;
        }
    }

    fn page(&mut self, ui: &mut Ui, class: &ResolvedClass) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&class.name).strong().size(15.0));
            ui.label(RichText::new(format!("({:?})", class.kind)).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.hyperlink_to(t!("entities.help_vdc"), vdc_url(&class.name));
            });
        });
        if !class.description.trim().is_empty() {
            ui.add(egui::Label::new(class.description.trim()).wrap());
        }
        crate::widgets::search_box(ui, &mut self.filter, t!("entities.help_filter"));
        let search = WordSearch::parse(&self.filter);
        ui.separator();
        ScrollArea::vertical().id_salt("hp_ui_entities.help_scroll").auto_shrink([false, false]).show(ui, |ui| {
            crate::widgets::fit_scroll_content(ui);
            let keys: Vec<_> = class
                .props
                .iter()
                .filter(|p| p.kind != hp_fgd::PropertyType::Flags)
                .filter(|p| search.matches2(&format!("{} {}", p.name, p.label()), &p.description))
                .collect();
            section(ui, &tf!("entities.help_keys", keys.len()), |ui| {
                egui::Grid::new("hp_ui_entities.help_keys").num_columns(3).striped(true).spacing([10.0, 4.0]).show(
                    ui,
                    |ui| {
                        for p in &keys {
                            ui.vertical(|ui| {
                                ui.set_width(NAME_COLUMN_W);
                                ui.add(egui::Label::new(RichText::new(p.label()).strong()).truncate());
                                ui.add(egui::Label::new(RichText::new(&p.name).monospace().small()).truncate());
                            });
                            ui.vertical(|ui| {
                                ui.set_width(TYPE_COLUMN_W);
                                ui.label(RichText::new(p.kind.to_string()).small());
                                if let Some(d) = p.default_value().filter(|d| !d.is_empty()) {
                                    ui.label(RichText::new(tf!("entities.default_value", d)).small().weak());
                                }
                            });
                            ui.add(egui::Label::new(RichText::new(p.description.trim()).small()).wrap());
                            ui.end_row();
                        }
                    },
                );
            });
            let flags: Vec<_> = class
                .props
                .iter()
                .filter(|p| p.kind == hp_fgd::PropertyType::Flags)
                .flat_map(|p| p.flags.iter())
                .filter(|f| search.matches(&f.display))
                .collect();
            if !flags.is_empty() {
                section(ui, &tf!("entities.help_flags", flags.len()), |ui| {
                    for f in flags {
                        let on = if f.default_on {
                            format!(" · {}", t!("entities.flag_default_on"))
                        } else {
                            String::new()
                        };
                        ui.label(format!("{:>6}  {}{on}", f.value, f.display));
                    }
                });
            }
            io_section(ui, &tf!("entities.help_inputs", class.inputs.len()), &class.inputs, &search);
            io_section(ui, &tf!("entities.help_outputs", class.outputs.len()), &class.outputs, &search);
        });
    }
}

fn section(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    egui::CollapsingHeader::new(RichText::new(title).strong()).default_open(true).show(ui, body);
}

fn io_section(ui: &mut Ui, title: &str, defs: &[IoDef], search: &WordSearch) {
    let shown: Vec<&IoDef> = defs.iter().filter(|d| search.matches2(&d.name, &d.description)).collect();
    if shown.is_empty() {
        return;
    }
    section(ui, title, |ui| {
        egui::Grid::new(Id::unique(("hp_ui_entities.help_io", title)))
            .num_columns(2)
            .striped(true)
            .spacing([10.0, 4.0])
            .show(ui, |ui| {
                for d in shown {
                    ui.vertical(|ui| {
                        ui.set_width(NAME_COLUMN_W);
                        ui.add(egui::Label::new(RichText::new(&d.name).strong()).truncate());
                        if d.arg_type != hp_fgd::PropertyType::Void {
                            ui.label(RichText::new(d.arg_type.to_string()).small().weak());
                        }
                    });
                    ui.add(egui::Label::new(RichText::new(d.description.trim()).small()).wrap());
                    ui.end_row();
                }
            });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_are_safe() {
        assert_eq!(vdc_url("light_spot"), "https://developer.valvesoftware.com/wiki/light_spot");
        assert_eq!(vdc_url("a/../b?x=1"), "https://developer.valvesoftware.com/wiki/abx1");
    }

    #[test]
    fn window_draws_known_and_unknown_classes() {
        crate::register_translations();
        let fgd = crate::test_fgd::test_fgd();
        let mut state = hp_app::testing::test_state();
        let mut w = HelpWindow::default();
        for class in ["light", "trigger_multiple", "no_such_class"] {
            state.runtime.entities.help_class = Some(class.to_owned());
            hp_app::testing::run_ctx(&mut state, |ctx, state| w.show(ctx, state, Some(&fgd), false));
            assert_eq!(state.runtime.entities.help_class.as_deref(), Some(class));
        }
        w.filter = "turn !off".into();
        state.runtime.entities.help_class = Some("light".into());
        hp_app::testing::run_ctx(&mut state, |ctx, state| w.show(ctx, state, Some(&fgd), false));
    }
}
