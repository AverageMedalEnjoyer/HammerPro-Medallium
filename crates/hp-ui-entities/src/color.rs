use egui::color_picker::{Alpha, color_picker_hsva_2d};
use egui::ecolor::Hsva;
use egui::{Color32, CornerRadius, DragValue, Id, RichText, Sense, Stroke, Ui, vec2};
use hp_app::{AppState, Severity, t, tf};
use hp_editor::Document;
use hp_fgd::{PropertyType, ResolvedFgd};

use crate::edit::{Op, apply};
use crate::values::Color255;
use crate::view::{SelectionSummary, Target};

pub(crate) const PALETTE_BLOCK: &str = "palette_plus";
pub(crate) const PALETTE_SIZE: usize = 16;
const COLOR_KEYS: [&str; 4] = ["_light", "rendercolor", "_ambient", "color"];
const LABEL_PALETTE: &str = "Edit palette";

pub(crate) fn color_key(doc: &Document, fgd: Option<&ResolvedFgd>) -> Option<(Vec<hp_editor::Id>, String)> {
    let ids = SelectionSummary::of(doc).entities;
    let first = doc.scene().entity(*ids.first()?)?;
    let e = &first.entity;
    let from_fgd =
        fgd.and_then(|f| f.class(&e.classname)).and_then(|c| c.props.iter().find(|p| p.kind == PropertyType::Color255));
    let key = match from_fgd {
        Some(p) => p.name.clone(),
        None => COLOR_KEYS.iter().find(|k| e.get(k).is_some())?.to_string(),
    };
    Some((ids, key))
}

pub(crate) fn read_palette(doc: &Document) -> [Option<[u8; 3]>; PALETTE_SIZE] {
    let mut out = [None; PALETTE_SIZE];
    if let Some(block) = doc.extra_block(PALETTE_BLOCK) {
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = block.get(&format!("color{i}")).and_then(Color255::parse).map(|c| c.to_srgb());
        }
    }
    out
}

pub(crate) fn store_palette(doc: &mut Document, i: usize, rgb: [u8; 3]) -> hp_editor::EditResult<()> {
    if i >= PALETTE_SIZE {
        return Ok(());
    }
    let mut block = doc.extra_block(PALETTE_BLOCK).cloned().unwrap_or_else(|| hp_kv::Block::new(PALETTE_BLOCK));
    block.set(format!("color{i}"), format!("{} {} {}", rgb[0], rgb[1], rgb[2]));
    doc.begin(LABEL_PALETTE);
    let r = doc.set_extra_block(PALETTE_BLOCK, block);
    if r.is_ok() {
        doc.commit();
    } else {
        doc.cancel();
    }
    r
}

#[derive(Clone, Debug, PartialEq)]
struct Editing {
    ids: Vec<hp_editor::Id>,
    key: String,
}

#[derive(Default)]
pub(crate) struct ColorWindow {
    editing: Option<Editing>,
    hsva: Option<Hsva>,
}

impl ColorWindow {
    pub fn show(&mut self, ctx: &egui::Context, state: &mut AppState, fgd: Option<&ResolvedFgd>) {
        if std::mem::take(&mut state.runtime.entities.edit_color) {
            let fgd = fgd.or_else(|| state.assets.fgd());
            match color_key(&state.doc, fgd) {
                Some((ids, key)) => {
                    self.editing = Some(Editing { ids, key });
                    self.hsva = None;
                }
                None => {
                    state.notify(Severity::Info, t!("entities.color_none"));
                    self.editing = None;
                }
            }
        }
        let Some(editing) = self.editing.clone() else { return };
        let ids: Vec<hp_editor::Id> =
            editing.ids.iter().copied().filter(|id| state.doc.scene().entity(*id).is_some()).collect();
        if ids.is_empty() {
            self.editing = None;
            return;
        }
        let value = state.doc.scene().entity(ids[0]).and_then(|n| n.entity.get(&editing.key)).unwrap_or("").to_owned();
        let parsed = Color255::parse(&value).unwrap_or(Color255 { rgb: [255.0; 3], brightness: None });
        let mut open = true;
        let mut ops = Vec::new();
        hp_app::chrome::Window::new(tf!("entities.color_title", editing.key))
            .id(Id::unique("hp_ui_entities.color_window"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .default_width(300.0)
            .show(ctx, |ui| {
                self.body(ui, state, &ids, &editing.key, parsed, &mut ops);
            });
        for op in ops {
            if let Err(e) = apply(&mut state.doc, None, op) {
                state.notify(Severity::Error, tf!("entities.edit_failed", e));
            }
        }
        if !open {
            self.editing = None;
        }
    }

    fn body(
        &mut self,
        ui: &mut Ui,
        state: &mut AppState,
        ids: &[hp_editor::Id],
        key: &str,
        parsed: Color255,
        ops: &mut Vec<Op>,
    ) {
        let target = Target::Entities(ids.to_vec());
        let srgb = parsed.to_srgb();
        let current = Hsva::from_srgb(srgb);
        let hsva = self.hsva.filter(|h| h.to_srgb() == srgb).unwrap_or(current);
        let mut edit = hsva;
        let mut new_rgb: Option<[u8; 3]> = None;
        if color_picker_hsva_2d(ui, &mut edit, Alpha::Opaque) {
            new_rgb = Some(edit.to_srgb());
        }
        ui.horizontal(|ui| {
            let mut rgb = srgb;
            for (i, label) in ["R", "G", "B"].iter().enumerate() {
                if ui.add(DragValue::new(&mut rgb[i]).range(0..=255).prefix(format!("{label} "))).changed() {
                    new_rgb = Some(rgb);
                    edit = Hsva::from_srgb(rgb);
                }
            }
        });
        ui.horizontal(|ui| {
            let (mut h, mut s, mut v) = (edit.h * 360.0, edit.s * 100.0, edit.v * 100.0);
            let mut changed = false;
            changed |=
                ui.add(DragValue::new(&mut h).range(0.0..=360.0).max_decimals(0).prefix("H ").suffix("°")).changed();
            changed |=
                ui.add(DragValue::new(&mut s).range(0.0..=100.0).max_decimals(0).prefix("S ").suffix("%")).changed();
            changed |=
                ui.add(DragValue::new(&mut v).range(0.0..=100.0).max_decimals(0).prefix("V ").suffix("%")).changed();
            if changed {
                edit = Hsva::new(h / 360.0, s / 100.0, v / 100.0, 1.0);
                new_rgb = Some(edit.to_srgb());
            }
        });
        let mut brightness = parsed.brightness;
        if let Some(b) = brightness.as_mut() {
            ui.horizontal(|ui| {
                ui.label(t!("entities.brightness"));
                let slider = egui::Slider::new(b, 0.0..=2000.0).logarithmic(true).max_decimals(0);
                if ui.add(slider).changed() {
                    new_rgb.get_or_insert(srgb);
                }
            });
        }
        self.hsva = Some(edit);
        if let Some(rgb) = new_rgb {
            let value = Color255 { rgb: parsed.rgb, brightness }.with_srgb(rgb).format();
            ops.push(Op::SetKey { target: target.clone(), key: key.to_owned(), value, merge: true });
        }
        ui.separator();
        ui.label(RichText::new(t!("entities.palette")).strong()).on_hover_text(t!("entities.palette_tip"));
        let palette = read_palette(&state.doc);
        let mut store: Option<usize> = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
            for (i, slot) in palette.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), Sense::CLICK);
                let fill = slot.map_or(Color32::TRANSPARENT, |c| Color32::from_rgb(c[0], c[1], c[2]));
                ui.painter().rect_filled(rect, CornerRadius::same(3), fill);
                let stroke =
                    if resp.hovered() { ui.visuals().strong_text_color() } else { ui.visuals().weak_text_color() };
                ui.painter().rect_stroke(
                    rect,
                    CornerRadius::same(3),
                    Stroke::new(1.0, stroke),
                    egui::StrokeKind::Inside,
                );
                let resp = resp.on_hover_text(t!("entities.palette_tip"));
                if resp.clicked()
                    && let Some(c) = slot
                {
                    let value = Color255 { rgb: parsed.rgb, brightness }.with_srgb(*c).format();
                    ops.push(Op::SetKey { target: target.clone(), key: key.to_owned(), value, merge: false });
                    self.hsva = None;
                }
                if resp.secondary_clicked() {
                    store = Some(i);
                }
            }
        });
        if let Some(i) = store
            && let Err(e) = store_palette(&mut state.doc, i, new_rgb.unwrap_or(srgb))
        {
            state.notify(Severity::Error, tf!("entities.edit_failed", e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_keys_of_the_selection() {
        let mut s = hp_app::testing::test_state();
        let fgd = crate::test_fgd::test_fgd();
        assert_eq!(color_key(&s.doc, Some(&fgd)), None, "nothing selected");
        s.doc.set_selection(&[hp_editor::ObjRef::Entity(5)]);
        assert_eq!(color_key(&s.doc, Some(&fgd)), Some((vec![5], "_light".to_owned())));
        assert_eq!(color_key(&s.doc, None), Some((vec![5], "_light".to_owned())), "known key without game data");
        s.doc.set_selection(&[hp_editor::ObjRef::Entity(4)]);
        assert_eq!(color_key(&s.doc, None), None);
    }

    #[test]
    fn palette_round_trips_in_the_map() {
        let mut s = hp_app::testing::test_state();
        assert_eq!(read_palette(&s.doc), [None; PALETTE_SIZE]);
        store_palette(&mut s.doc, 3, [10, 20, 30]).unwrap();
        let p = read_palette(&s.doc);
        assert_eq!(p[3], Some([10, 20, 30]));
        assert_eq!(s.doc.undo_label(), Some(LABEL_PALETTE));
        assert!(s.doc.to_vmf_string().contains("palette_plus"));
        store_palette(&mut s.doc, 99, [1, 1, 1]).unwrap();
        assert!(s.doc.undo());
        assert_eq!(read_palette(&s.doc)[3], None);
    }

    #[test]
    fn window_edits_live_in_one_step() {
        crate::register_translations();
        let mut s = hp_app::testing::test_state();
        let fgd = crate::test_fgd::test_fgd();
        let mut w = ColorWindow::default();
        s.runtime.entities.edit_color = true;
        hp_app::testing::run_ctx(&mut s, |ctx, state| w.show(ctx, state, Some(&fgd)));
        assert!(w.editing.is_none(), "nothing selected");
        s.doc.set_selection(&[hp_editor::ObjRef::Entity(5)]);
        s.runtime.entities.edit_color = true;
        hp_app::testing::run_ctx(&mut s, |ctx, state| w.show(ctx, state, Some(&fgd)));
        assert_eq!(w.editing.as_ref().map(|e| e.key.as_str()), Some("_light"));
        assert_eq!(s.doc.history_len().0, 0, "drawing does not edit");
    }
}
