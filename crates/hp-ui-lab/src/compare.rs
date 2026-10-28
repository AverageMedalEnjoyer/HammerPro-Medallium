use std::path::{Path, PathBuf};
use std::time::Duration;

use crossbeam_channel::Receiver;
use egui::RichText;
use hp_app::{Action, AppState, t, tf};
use hp_editor::diff::{Change, Detail, MapDiff, ObjectDiff, ObjectKind, diff_maps};
use hp_editor::{ObjRef, SelectOp};

const MAX_VMF: u64 = 1 << 30;

pub struct CompareJob {
    pub path: PathBuf,
    rx: Receiver<Result<MapDiff, String>>,
}

impl CompareJob {
    pub fn start(here: hp_vmf::Map, path: PathBuf, ctx: Option<egui::Context>) -> Option<CompareJob> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let worker_path = path.clone();
        std::thread::Builder::new()
            .name("hp-compare".to_owned())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let len = std::fs::metadata(&worker_path).map_err(|e| e.to_string())?.len();
                    if len > MAX_VMF {
                        return Err("the file is too big".to_owned());
                    }
                    let there = hp_vmf::load(&worker_path).map_err(|e| e.to_string())?;
                    Ok(diff_maps(&here, &there))
                }))
                .unwrap_or_else(|_| Err("the map could not be read".to_owned()));
                let _ = tx.send(result);
                if let Some(ctx) = ctx {
                    ctx.request_repaint();
                }
            })
            .ok()?;
        Some(CompareJob { path, rx })
    }

    pub fn poll(&self) -> Option<Result<MapDiff, String>> {
        match self.rx.try_recv() {
            Ok(r) => Some(r),
            Err(crossbeam_channel::TryRecvError::Empty) => None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => Some(Err("the comparison stopped".to_owned())),
        }
    }

    pub fn wait(&self, timeout: Duration) -> Option<Result<MapDiff, String>> {
        self.rx.recv_timeout(timeout).ok()
    }
}

#[derive(Default)]
pub struct CompareWindow {
    pub open: bool,
    pub other: Option<PathBuf>,
    diff: Option<MapDiff>,
    filter: String,
    show_here: bool,
    show_there: bool,
    show_changed: bool,
    rows: Vec<usize>,
    rows_key: Option<(String, bool, bool, bool, usize)>,
}

fn change_text(c: Change) -> &'static str {
    match c {
        Change::OnlyHere => t!("lab.compare.only_here"),
        Change::OnlyThere => t!("lab.compare.only_there"),
        Change::Changed => t!("lab.compare.changed"),
    }
}

fn change_color(c: Change) -> egui::Color32 {
    match c {
        Change::OnlyHere => hp_app::theme::success(),
        Change::OnlyThere => hp_app::theme::error(),
        Change::Changed => hp_app::theme::warning(),
    }
}

fn detail_text(d: &Detail) -> String {
    let none = || t!("lab.compare.none").to_owned();
    match d {
        Detail::Key { key, here, there } => {
            tf!("lab.compare.key", key, there.clone().unwrap_or_else(none), here.clone().unwrap_or_else(none))
        }
        Detail::Class { here, there } => tf!("lab.compare.class", there, here),
        Detail::Outputs { here, there } => tf!("lab.compare.outputs", there, here),
        Detail::Solids { here, there } => tf!("lab.compare.solids", there, here),
        Detail::Geometry => t!("lab.compare.geometry").to_owned(),
        Detail::Materials { here, there } => tf!("lab.compare.materials", there, here),
        Detail::Alignment => t!("lab.compare.alignment").to_owned(),
        Detail::Displacement => t!("lab.compare.displacement").to_owned(),
        Detail::Editor => t!("lab.compare.editor").to_owned(),
    }
}

fn object_of(o: &ObjectDiff) -> Option<ObjRef> {
    match (o.change, o.kind) {
        (Change::OnlyThere, _) => None,
        (_, ObjectKind::Entity) if o.id == 1 => None,
        (_, ObjectKind::Entity) => Some(ObjRef::Entity(o.id)),
        (_, ObjectKind::Solid) => Some(ObjRef::Solid(o.id)),
    }
}

impl CompareWindow {
    pub fn set_result(&mut self, path: PathBuf, diff: MapDiff) {
        self.other = Some(path);
        self.diff = Some(diff);
        self.open = true;
        self.show_here = true;
        self.show_there = true;
        self.show_changed = true;
        self.rows_key = None;
    }

    pub fn diff(&self) -> Option<&MapDiff> {
        self.diff.as_ref()
    }

    fn refresh_rows(&mut self) {
        let Some(diff) = &self.diff else {
            self.rows.clear();
            return;
        };
        let key = (self.filter.clone(), self.show_here, self.show_there, self.show_changed, diff.objects.len());
        if self.rows_key.as_ref() == Some(&key) {
            return;
        }
        let q = self.filter.trim().to_ascii_lowercase();
        self.rows = diff
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| match o.change {
                Change::OnlyHere => self.show_here,
                Change::OnlyThere => self.show_there,
                Change::Changed => self.show_changed,
            })
            .filter(|(_, o)| {
                q.is_empty()
                    || o.class.to_ascii_lowercase().contains(&q)
                    || o.id.to_string() == q
                    || o.details.iter().any(|d| detail_text(d).to_ascii_lowercase().contains(&q))
            })
            .map(|(i, _)| i)
            .collect();
        self.rows_key = Some(key);
    }

    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState) {
        if !self.open || self.diff.is_none() {
            return;
        }
        let mut open = true;
        let title = format!("⚖  {}", t!("lab.compare.title"));
        hp_app::chrome::Window::new(title)
            .id(egui::Id::unique("lab_compare_maps"))
            .open(&mut open)
            .default_size([620.0, 480.0])
            .min_size([360.0, 200.0])
            .resizable(true)
            .show(ctx, |ui| self.ui(ui, state));
        if !open {
            self.open = false;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, state: &mut AppState) {
        let name = self
            .other
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Some(diff) = &self.diff else { return };
        ui.label(tf!("lab.compare.with", name));
        if diff.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(t!("lab.compare.same")).color(hp_app::theme::success()));
            return;
        }
        let (here, there, changed) =
            (diff.count(Change::OnlyHere), diff.count(Change::OnlyThere), diff.count(Change::Changed));
        ui.horizontal_wrapped(|ui| {
            ui.checkbox(
                &mut self.show_here,
                RichText::new(tf!("lab.compare.count_here", here)).color(change_color(Change::OnlyHere)),
            );
            ui.checkbox(
                &mut self.show_there,
                RichText::new(tf!("lab.compare.count_there", there)).color(change_color(Change::OnlyThere)),
            );
            ui.checkbox(
                &mut self.show_changed,
                RichText::new(tf!("lab.compare.count_changed", changed)).color(change_color(Change::Changed)),
            );
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.filter).hint_text(t!("lab.compare.filter")).desired_width(220.0),
            );
            if ui.button(t!("lab.compare.select_all")).on_hover_text(t!("lab.compare.select_all_tip")).clicked() {
                self.refresh_rows();
                if let Some(diff) = &self.diff {
                    let objs: Vec<ObjRef> = self.rows.iter().filter_map(|&i| object_of(&diff.objects[i])).collect();
                    state.doc.select(&objs, SelectOp::Replace);
                }
            }
        });
        ui.separator();
        self.refresh_rows();
        let Some(diff) = &self.diff else { return };
        let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
        let mut action: Option<(ObjRef, bool)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, self.rows.len(), |ui, range| {
            for &i in &self.rows[range] {
                let o = &diff.objects[i];
                ui.horizontal(|ui| {
                    ui.label(RichText::new(change_text(o.change)).color(change_color(o.change)).small());
                    let kind = match o.kind {
                        ObjectKind::Entity => t!("lab.compare.entity"),
                        ObjectKind::Solid => t!("lab.compare.solid"),
                    };
                    ui.label(format!("{kind} {}", o.id));
                    ui.label(RichText::new(&o.class).weak());
                    let summary: Vec<String> = o.details.iter().take(3).map(detail_text).collect();
                    let more = o.details.len().saturating_sub(3);
                    let mut text = summary.join(", ");
                    if more > 0 {
                        text.push_str(&tf!("lab.compare.more", more));
                    }
                    let r = ui.add(egui::Label::new(RichText::new(text).small()).truncate());
                    if o.details.len() > 1 {
                        let all: Vec<String> = o.details.iter().map(detail_text).collect();
                        r.on_hover_text(all.join("\n"));
                    }
                    if let Some(obj) = object_of(o)
                        && state.doc.scene().contains(obj)
                    {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button(t!("lab.compare.go")).on_hover_text(t!("lab.compare.go_tip")).clicked() {
                                action = Some((obj, true));
                            }
                            if ui.small_button(t!("lab.compare.select")).clicked() {
                                action = Some((obj, false));
                            }
                        });
                    }
                });
            }
        });
        if let Some((obj, go)) = action {
            state.doc.select(&[obj], SelectOp::Replace);
            if go {
                state.request(Action::Center2dOnSelection);
                state.request(Action::Center3dOnSelection);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comparing_with_a_changed_copy() {
        crate::register_translations();
        let mut state = hp_app::testing::test_state();
        let dir = std::env::temp_dir().join(format!("hp_lab_compare_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let other = dir.join("other.vmf");
        std::fs::write(&other, hp_app::testing::TEST_VMF).expect("write");
        state.doc.set_entity_key(&[5], "origin", "0 0 200").expect("edit");
        let job = CompareJob::start(state.doc.to_map(), other.clone(), None).expect("job");
        let diff = job.wait(Duration::from_secs(30)).expect("answer").expect("diff");
        assert_eq!(diff.count(Change::Changed), 1);
        let mut window = CompareWindow::default();
        window.set_result(other.clone(), diff);
        hp_app::testing::run_ctx(&mut state, |ctx, st| window.show(ctx, st));
        assert!(window.open);
        let job = CompareJob::start(state.doc.to_map(), dir.join("missing.vmf"), None).expect("job");
        assert!(job.wait(Duration::from_secs(30)).expect("answer").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
