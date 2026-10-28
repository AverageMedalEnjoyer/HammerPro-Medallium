use std::f32::consts::PI;

use egui::{Color32, Pos2, Rect, Vec2, pos2};
use hp_app::settings::UiSettings;

const MAX_SIDE: u32 = 4096;

pub const MAX_OPACITY: f32 = 0.5;

#[derive(Default)]
pub struct BackgroundImage {
    loaded: Option<(String, Option<egui::TextureHandle>)>,
}

impl std::fmt::Debug for BackgroundImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BackgroundImage").field("path", &self.loaded.as_ref().map(|(p, _)| p)).finish()
    }
}

impl BackgroundImage {
    pub fn paint(&mut self, ctx: &egui::Context, s: &UiSettings, holes: &[Rect]) {
        let path = s.background_image.trim();
        if path.is_empty() {
            self.loaded = None;
            return;
        }
        if self.loaded.as_ref().is_none_or(|(p, _)| p != path) {
            let texture = load(ctx, path);
            if texture.is_none() {
                log::warn!("background image {path} could not be read");
            }
            self.loaded = Some((path.to_owned(), texture));
        }
        let Some((_, Some(texture))) = &self.loaded else { return };
        let opacity = if s.background_opacity.is_finite() { s.background_opacity.clamp(0.0, MAX_OPACITY) } else { 0.0 };
        let alpha = (opacity * 255.0).round() as u8;
        if alpha == 0 {
            return;
        }
        let (screen, radius) = match hp_app::chrome::last_geometry(ctx, ctx.viewport_id()) {
            Some(g) => (g.visible, g.radius),
            None => (ctx.content_rect(), 0.0),
        };
        let outline = rounded_outline(screen, radius);
        let offset = Vec2::new(s.background_offset[0], s.background_offset[1]);
        let rect = fit_rect(screen, texture.size_vec2(), s.background_zoom, offset);
        let layer = egui::LayerId::background();
        let painter = ctx.layer_painter(layer);
        let tint = Color32::from_white_alpha(alpha);
        let mut mesh = egui::Mesh::with_texture(texture.id());
        for piece in subtract(screen, holes) {
            add_polygon(&mut mesh, &clip_to_rect(&outline, piece), rect, tint);
        }
        if !mesh.is_empty() {
            painter.add(egui::Shape::mesh(mesh));
        }
    }
}

pub fn rounded_outline(rect: Rect, radius: f32) -> Vec<Pos2> {
    let radius = if radius.is_finite() { radius.clamp(0.0, rect.width().min(rect.height()) / 2.0) } else { 0.0 };
    if radius <= 0.0 {
        return vec![rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()];
    }
    const STEPS: usize = 8;
    let r = radius;
    let corners = [
        (pos2(rect.left() + r, rect.top() + r), PI),
        (pos2(rect.right() - r, rect.top() + r), 1.5 * PI),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 0.5 * PI),
    ];
    let mut out = Vec::with_capacity(4 * (STEPS + 1));
    for (c, start) in corners {
        for i in 0..=STEPS {
            let a = start + 0.5 * PI * i as f32 / STEPS as f32;
            out.push(c + r * Vec2::new(a.cos(), a.sin()));
        }
    }
    out
}

pub fn clip_to_rect(poly: &[Pos2], rect: Rect) -> Vec<Pos2> {
    let mut out = poly.to_vec();
    let sides: [&dyn Fn(Pos2) -> f32; 4] =
        [&|p| p.x - rect.left(), &|p| rect.right() - p.x, &|p| p.y - rect.top(), &|p| rect.bottom() - p.y];
    for inside in sides {
        if out.is_empty() {
            break;
        }
        let input = std::mem::take(&mut out);
        for (i, &b) in input.iter().enumerate() {
            let a = input[(i + input.len() - 1) % input.len()];
            let (da, db) = (inside(a), inside(b));
            if (da >= 0.0) != (db >= 0.0) {
                out.push(a + (b - a) * (da / (da - db)));
            }
            if db >= 0.0 {
                out.push(b);
            }
        }
    }
    out
}

fn add_polygon(mesh: &mut egui::Mesh, poly: &[Pos2], image: Rect, color: Color32) {
    if poly.len() < 3 || image.width() <= 0.0 || image.height() <= 0.0 {
        return;
    }
    let base = mesh.vertices.len() as u32;
    for &p in poly {
        let uv = pos2((p.x - image.left()) / image.width(), (p.y - image.top()) / image.height());
        mesh.vertices.push(egui::epaint::Vertex { pos: p, uv, color });
    }
    for i in 1..poly.len() as u32 - 1 {
        mesh.indices.extend_from_slice(&[base, base + i, base + i + 1]);
    }
}

pub fn subtract(rect: Rect, holes: &[Rect]) -> Vec<Rect> {
    let mut pieces = vec![rect];
    for hole in holes {
        let mut next = Vec::with_capacity(pieces.len() + 3);
        for p in pieces {
            let i = p.intersect(*hole);
            if !i.is_positive() {
                next.push(p);
                continue;
            }
            if i.min.y > p.min.y {
                next.push(Rect::from_min_max(p.min, egui::pos2(p.max.x, i.min.y)));
            }
            if i.max.y < p.max.y {
                next.push(Rect::from_min_max(egui::pos2(p.min.x, i.max.y), p.max));
            }
            if i.min.x > p.min.x {
                next.push(Rect::from_min_max(egui::pos2(p.min.x, i.min.y), egui::pos2(i.min.x, i.max.y)));
            }
            if i.max.x < p.max.x {
                next.push(Rect::from_min_max(egui::pos2(i.max.x, i.min.y), egui::pos2(p.max.x, i.max.y)));
            }
        }
        pieces = next;
    }
    pieces
}

pub fn fit_rect(screen: Rect, image: Vec2, zoom: f32, offset: Vec2) -> Rect {
    if image.x <= 0.0 || image.y <= 0.0 {
        return screen;
    }
    let zoom = if zoom.is_finite() && zoom > 0.0 { zoom } else { 1.0 };
    let cover = (screen.width() / image.x).max(screen.height() / image.y) * zoom;
    let offset = if offset.is_finite() { offset } else { Vec2::ZERO };
    Rect::from_center_size(screen.center() + offset, image * cover)
}

fn load(ctx: &egui::Context, path: &str) -> Option<egui::TextureHandle> {
    let img = image::open(path).ok()?;
    let img = if img.width().max(img.height()) > MAX_SIDE { img.thumbnail(MAX_SIDE, MAX_SIDE) } else { img };
    let rgba = img.to_rgba8();
    let size = [rgba.width() as usize, rgba.height() as usize];
    let color = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
    Some(ctx.load_texture("hp_background_image", color, egui::TextureOptions::LINEAR))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_image_covers_the_window_centered_then_moves_and_zooms() {
        let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(1600.0, 900.0));
        let r = fit_rect(screen, egui::vec2(100.0, 100.0), 1.0, Vec2::ZERO);
        assert_eq!(r.width(), 1600.0);
        assert_eq!(r.center(), screen.center());
        assert!(r.min.y < 0.0 && r.max.y > 900.0);
        let r = fit_rect(screen, egui::vec2(100.0, 100.0), 2.0, egui::vec2(10.0, -20.0));
        assert_eq!(r.width(), 3200.0);
        assert_eq!(r.center(), screen.center() + egui::vec2(10.0, -20.0));
        let r = fit_rect(screen, egui::vec2(160.0, 90.0), f32::NAN, egui::vec2(f32::NAN, 0.0));
        assert_eq!(r, screen);
    }

    #[test]
    fn holes_are_cut_out() {
        let screen = Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let hole = Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(30.0, 40.0));
        let pieces = subtract(screen, &[hole]);
        let area: f32 = pieces.iter().map(|p| p.area()).sum();
        assert_eq!(area, 100.0 * 100.0 - 30.0 * 40.0);
        assert!(pieces.iter().all(|p| !p.intersect(hole).is_positive()));
        let far = Rect::from_min_size(egui::pos2(500.0, 500.0), egui::vec2(10.0, 10.0));
        assert_eq!(subtract(screen, &[far]), vec![screen]);
    }

    fn image_mesh(window: Rect, radius: f32, holes: &[Rect]) -> egui::Mesh {
        let outline = rounded_outline(window, radius);
        let mut mesh = egui::Mesh::default();
        for piece in subtract(window, holes) {
            add_polygon(&mut mesh, &clip_to_rect(&outline, piece), window, Color32::WHITE);
        }
        mesh
    }

    fn covers(mesh: &egui::Mesh, p: Pos2) -> bool {
        mesh.indices.chunks(3).any(|t| {
            let [a, b, c] = [t[0], t[1], t[2]].map(|i| mesh.vertices[i as usize].pos);
            let s = |u: Pos2, v: Pos2| (v - u).x * (p - u).y - (v - u).y * (p - u).x;
            let (x, y, z) = (s(a, b), s(b, c), s(c, a));
            (x >= 0.0 && y >= 0.0 && z >= 0.0) || (x <= 0.0 && y <= 0.0 && z <= 0.0)
        })
    }

    #[test]
    fn round_corners_stay_clear() {
        let window = Rect::from_min_size(egui::pos2(8.0, 8.0), Vec2::new(400.0, 300.0));
        let mesh = image_mesh(window, 8.0, &[]);
        for corner in [window.left_top(), window.right_top(), window.left_bottom(), window.right_bottom()] {
            let outside = corner + (window.center() - corner).normalized();
            assert!(!covers(&mesh, outside), "{outside:?} is in a cut-out");
            let dir = window.center() - corner;
            let along = corner + Vec2::new(dir.x.signum() * 9.0, dir.y.signum() * 0.5);
            assert!(covers(&mesh, along), "{along:?} is in the window");
        }
        assert!(covers(&mesh, window.center()));
        assert!(covers(&mesh, window.left_top() + Vec2::splat(4.0)));
        let hole = Rect::from_min_size(egui::pos2(100.0, 100.0), Vec2::splat(50.0));
        let holed = image_mesh(window, 8.0, &[hole]);
        assert!(!covers(&holed, hole.center()));
        assert!(covers(&holed, window.center() + Vec2::new(0.0, 120.0)));
        assert_eq!(rounded_outline(window, 0.0).len(), 4);
        assert!(covers(&image_mesh(window, 0.0, &[]), window.left_top() + Vec2::splat(0.5)));
    }
}
