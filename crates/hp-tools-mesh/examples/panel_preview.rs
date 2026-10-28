use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use hp_app::Tool;
use hp_app::state::AppState;
use hp_editor::{ObjRef, SelectOp};
use hp_tools_mesh::{DisplacementTool, Interaction, VertexTool};

struct Preview {
    state: AppState,
    vertex: VertexTool,
    disp: DisplacementTool,
    frames: u32,
    out: PathBuf,
    saved: Arc<Mutex<bool>>,
}

impl eframe::App for Preview {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if self.frames == 0 {
            hp_app::theme::apply(&ctx, hp_app::settings::ThemeChoice::Dark, 1.0);
        }
        egui::Panel::top("hp_preview_options").show(ui, |ui| {
            ui.add_space(2.0);
            self.vertex.options_ui(ui, &mut self.state);
            ui.add_space(2.0);
        });
        egui::Panel::right("hp_preview_tool").default_size(300.0).show(ui, |ui| {
            self.disp.options_ui(ui, &mut self.state);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui.label(self.vertex.status_hint(&self.state).unwrap_or_default());
            ui.label(self.disp.status_hint(&self.state).unwrap_or_default());
            let area = ui.available_rect_before_wrap();
            let (left, right) = area.split_left_right_at_fraction(0.5);
            let front = hp_app::ViewInfo {
                index: 2,
                kind: hp_app::ViewKind::Front,
                rect: left.shrink(4.0),
                cam3d: hp_app::view::Camera3d::default(),
                cam2d: hp_app::view::Camera2d { center: glam::DVec2::new(264.0, 64.0), zoom: 1.6 },
                pixels_per_point: 1.0,
                grid: 16.0,
            };
            let mut cam = hp_app::view::Camera3d { pos: glam::DVec3::new(420.0, -260.0, 300.0), ..Default::default() };
            cam.look_at(glam::DVec3::new(128.0, 264.0, 64.0));
            let persp = hp_app::ViewInfo {
                index: 0,
                kind: hp_app::ViewKind::Perspective,
                rect: right.shrink(4.0),
                cam3d: cam,
                cam2d: hp_app::view::Camera2d::default(),
                pixels_per_point: 1.0,
                grid: 16.0,
            };
            for view in [front, persp] {
                draw_view(ui, &self.state, &self.vertex, &view);
            }
        });
        self.frames += 1;
        if self.frames == 6 {
            let out = self.out.clone();
            let saved = self.saved.clone();
            ctx.request_screenshot(move |image| {
                let [w, h] = image.size;
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                match image::RgbaImage::from_raw(w as u32, h as u32, bytes) {
                    Some(img) => {
                        if let Err(e) = img.save(&out) {
                            eprintln!("could not save {}: {e}", out.display());
                        }
                    }
                    None => eprintln!("bad screenshot buffer"),
                }
                if let Ok(mut s) = saved.lock() {
                    *s = true;
                }
            });
        }
        if self.saved.lock().map(|s| *s).unwrap_or(true) || self.frames > 600 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        ctx.request_repaint();
    }
}

fn draw_view(ui: &egui::Ui, state: &AppState, tool: &dyn Tool, view: &hp_app::ViewInfo) {
    let painter = ui.painter().with_clip_rect(view.rect);
    painter.rect_filled(view.rect, 0.0, egui::Color32::from_rgb(20, 20, 22));
    let edge = egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 125, 135));
    for node in state.doc.scene().solids().values() {
        for [a, b] in node.geom.edges() {
            if let (Some(sa), Some(sb)) = (view.world_to_screen(a), view.world_to_screen(b)) {
                painter.line_segment([sa, sb], edge);
            }
        }
    }
    let mut overlay = hp_render::Overlay::default();
    tool.overlay(state, view, &mut overlay);
    let color = |c: [f32; 4]| {
        egui::Color32::from_rgba_unmultiplied(
            (c[0] * 255.0) as u8,
            (c[1] * 255.0) as u8,
            (c[2] * 255.0) as u8,
            (c[3] * 255.0) as u8,
        )
    };
    let world = |p: [f32; 3]| glam::DVec3::new(f64::from(p[0]), f64::from(p[1]), f64::from(p[2]));
    for l in overlay.lines.iter().chain(&overlay.lines_on_top) {
        if let (Some(a), Some(b)) = (view.world_to_screen(world(l.a)), view.world_to_screen(world(l.b))) {
            painter.line_segment([a, b], egui::Stroke::new(1.5, color(l.color)));
        }
    }
    for p in &overlay.points {
        if let Some(s) = view.world_to_screen(world(p.pos)) {
            let r = egui::Rect::from_center_size(s, egui::vec2(p.size_px, p.size_px));
            painter.rect_filled(r, 0.0, color(p.color));
            painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, egui::Color32::BLACK), egui::StrokeKind::Inside);
        }
    }
    tool.paint(state, view, &painter);
}

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    let out = PathBuf::from(args.get(1).cloned().unwrap_or_else(|| "panel_preview.png".to_owned()));
    let french = args.iter().any(|a| a == "fr");
    hp_app::register_translations();
    hp_tools_mesh::register_translations();
    hp_app::i18n::set_lang(if french { hp_app::i18n::Lang::Fr } else { hp_app::i18n::Lang::En });

    let mut state = hp_app::testing::test_state();
    state.doc.select(&[ObjRef::Solid(2)], SelectOp::Replace);
    let mut disp = DisplacementTool::new();
    disp.create(&mut state, 3);
    state.doc.select(&[ObjRef::Solid(2), ObjRef::Solid(3)], SelectOp::Replace);
    let mut vertex = VertexTool::new();
    vertex.activate(&mut state);
    vertex.select_all_vertices(&state);
    if args.iter().any(|a| a == "alpha") {
        disp.interaction = Interaction::Alpha;
    } else if args.iter().any(|a| a == "select") {
        disp.interaction = Interaction::Select;
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 720.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Hammer Pro tool preview",
        options,
        Box::new(move |_cc| {
            Ok(Box::new(Preview { state, vertex, disp, frames: 0, out, saved: Arc::new(Mutex::new(false)) }))
        }),
    )
}
