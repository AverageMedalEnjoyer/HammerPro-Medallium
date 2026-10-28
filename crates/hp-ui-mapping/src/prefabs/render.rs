use glam::{DVec2, DVec3};
use hp_geom::Aabb;

const MAX_TRIANGLES: usize = 400_000;
const SS: usize = 2;
const POINT_HALF: f64 = 8.0;

fn view_dir() -> DVec3 {
    DVec3::new(0.55, 0.75, -0.62).normalize()
}

fn light_dir() -> DVec3 {
    DVec3::new(-0.35, -0.55, 1.0).normalize()
}

fn fnv(text: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for b in text.bytes() {
        h ^= u32::from(b.to_ascii_lowercase());
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

pub(crate) fn material_color(material: &str) -> Option<[f32; 3]> {
    let m = material.trim().replace('\\', "/").to_ascii_lowercase();
    let m = m.strip_prefix("materials/").unwrap_or(&m);
    if let Some(tool) = m.strip_prefix("tools/") {
        let c = match tool {
            "toolshint" | "toolsskip" => return None,
            "toolsnodraw" => [0.82, 0.76, 0.32],
            "toolstrigger" => [0.92, 0.55, 0.18],
            "toolsclip" | "toolsplayerclip" | "toolsnpcclip" | "toolsgrenadeclip" => [0.66, 0.38, 0.82],
            "toolsskybox" | "toolsskybox2d" => [0.45, 0.68, 0.92],
            "toolsblack" => [0.12, 0.12, 0.12],
            "toolsareaportal" => [0.88, 0.3, 0.3],
            "toolsinvisible" => [0.72, 0.72, 0.86],
            _ => [0.8, 0.45, 0.8],
        };
        return Some(c);
    }
    let h = fnv(m);
    let t = |shift: u32| ((h >> shift) & 0xFF) as f32 / 255.0 * 0.12 - 0.06;
    Some([0.72 + t(0), 0.74 + t(8), 0.77 + t(16)])
}

#[derive(Clone, Copy, Debug)]
struct Tri {
    p: [DVec3; 3],
    color: [f32; 3],
    normal: DVec3,
}

fn collect(frag: &hp_vmf::Fragment) -> (Vec<Tri>, Vec<[DVec3; 2]>) {
    let mut tris = Vec::new();
    let mut edges = Vec::new();
    let push_solid =
        |solid: &hp_vmf::Solid, tint: Option<[u8; 3]>, tris: &mut Vec<Tri>, edges: &mut Vec<[DVec3; 2]>| {
            let geom = hp_geom::build_solid(&solid.sides);
            if geom.aabb.is_empty() || !geom.aabb.min.is_finite() || !geom.aabb.max.is_finite() {
                return;
            }
            for face in &geom.faces {
                let material = solid.sides.get(face.side_index).map_or("", |s| s.material.as_str());
                let Some(mut color) = material_color(material) else { continue };
                if let Some(t) = tint {
                    for k in 0..3 {
                        color[k] = color[k] * 0.55 + f32::from(t[k]) / 255.0 * 0.45;
                    }
                }
                let n = face.normal();
                for [a, b, c] in face.triangles() {
                    if tris.len() >= MAX_TRIANGLES {
                        return;
                    }
                    let (Some(&a), Some(&b), Some(&c)) =
                        (face.vertices.get(a), face.vertices.get(b), face.vertices.get(c))
                    else {
                        continue;
                    };
                    tris.push(Tri { p: [a, b, c], color, normal: n });
                }
                let k = face.vertices.len();
                for i in 0..k {
                    if let (Some(&a), Some(&b)) = (face.vertices.get(i), face.vertices.get((i + 1) % k)) {
                        edges.push([a, b]);
                    }
                }
            }
        };
    for s in &frag.solids {
        push_solid(s, None, &mut tris, &mut edges);
    }
    for e in &frag.entities {
        if e.solids.is_empty() {
            let Some(o) = e.origin().filter(|o| o.is_finite()) else { continue };
            let b = Aabb::new(o - DVec3::splat(POINT_HALF), o + DVec3::splat(POINT_HALF));
            push_box(&b, e.editor.color, &mut tris, &mut edges);
        } else {
            for s in &e.solids {
                push_solid(s, Some(e.editor.color), &mut tris, &mut edges);
            }
        }
    }
    (tris, edges)
}

fn push_box(b: &Aabb, color: [u8; 3], tris: &mut Vec<Tri>, edges: &mut Vec<[DVec3; 2]>) {
    let c = b.corners();
    let color = color.map(|v| f32::from(v) / 255.0);
    let faces: [([usize; 4], DVec3); 6] = [
        ([0, 2, 6, 4], DVec3::NEG_X),
        ([1, 5, 7, 3], DVec3::X),
        ([0, 4, 5, 1], DVec3::NEG_Y),
        ([2, 3, 7, 6], DVec3::Y),
        ([0, 1, 3, 2], DVec3::NEG_Z),
        ([4, 6, 7, 5], DVec3::Z),
    ];
    for (f, n) in faces {
        if tris.len() + 2 > MAX_TRIANGLES {
            return;
        }
        tris.push(Tri { p: [c[f[0]], c[f[1]], c[f[2]]], color, normal: n });
        tris.push(Tri { p: [c[f[0]], c[f[2]], c[f[3]]], color, normal: n });
        for i in 0..4 {
            edges.push([c[f[i]], c[f[(i + 1) % 4]]]);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Thumbnail {
    pub size: usize,
    pub rgba: Vec<u8>,
}

pub(crate) fn render_text(text: &str, size: usize) -> Option<Thumbnail> {
    let frag = hp_vmf::parse_fragment_full(text).ok()?;
    render(&frag, size)
}

pub(crate) fn render(frag: &hp_vmf::Fragment, size: usize) -> Option<Thumbnail> {
    let size = size.clamp(8, 512);
    let (tris, edges) = collect(frag);
    if tris.is_empty() {
        return None;
    }
    let d = view_dir();
    let right = d.cross(DVec3::Z).normalize();
    let up = right.cross(d);
    let project = |p: DVec3| (DVec2::new(p.dot(right), p.dot(up)), p.dot(d));
    let (mut lo, mut hi) = (DVec2::splat(f64::INFINITY), DVec2::splat(f64::NEG_INFINITY));
    let (mut zlo, mut zhi) = (f64::INFINITY, f64::NEG_INFINITY);
    for t in &tris {
        for p in t.p {
            let (q, z) = project(p);
            lo = lo.min(q);
            hi = hi.max(q);
            zlo = zlo.min(z);
            zhi = zhi.max(z);
        }
    }
    if !(lo.is_finite() && hi.is_finite() && zlo.is_finite() && zhi.is_finite()) {
        return None;
    }
    let big = size * SS;
    let extent = (hi - lo).max_element().max(1e-6);
    let scale = big as f64 * 0.88 / extent;
    let center = (lo + hi) * 0.5;
    let half = big as f64 * 0.5;
    let to_screen = |p: DVec3| {
        let (q, z) = project(p);
        DVec3::new(half + (q.x - center.x) * scale, half - (q.y - center.y) * scale, z)
    };
    let mut depth = vec![f64::INFINITY; big * big];
    let mut color = vec![[0f32; 4]; big * big];
    let light = light_dir();
    for t in &tris {
        if t.normal.dot(d) > 1e-6 {
            continue;
        }
        let shade = (0.42 + 0.58 * t.normal.dot(light).max(0.0) + 0.12 * (-t.normal.dot(d)).max(0.0)).min(1.15) as f32;
        let rgb = t.color.map(|c| (c * shade).clamp(0.0, 1.0));
        let s = t.p.map(to_screen);
        fill_triangle(&s, big, &mut depth, &mut color, [rgb[0], rgb[1], rgb[2], 1.0]);
    }
    let bias = (zhi - zlo).max(1.0) * 0.004;
    for [a, b] in &edges {
        let (sa, sb) = (to_screen(*a), to_screen(*b));
        draw_line(sa, sb, big, &depth, &mut color, bias);
    }
    Some(downsample(&color, big, size))
}

fn fill_triangle(s: &[DVec3; 3], big: usize, depth: &mut [f64], color: &mut [[f32; 4]], rgba: [f32; 4]) {
    let area = (s[1].x - s[0].x) * (s[2].y - s[0].y) - (s[2].x - s[0].x) * (s[1].y - s[0].y);
    if area.abs() < 1e-12 || !area.is_finite() {
        return;
    }
    let max = big as f64 - 1.0;
    let x0 = s.iter().map(|p| p.x).fold(f64::INFINITY, f64::min).floor().clamp(0.0, max) as usize;
    let x1 = s.iter().map(|p| p.x).fold(f64::NEG_INFINITY, f64::max).ceil().clamp(0.0, max) as usize;
    let y0 = s.iter().map(|p| p.y).fold(f64::INFINITY, f64::min).floor().clamp(0.0, max) as usize;
    let y1 = s.iter().map(|p| p.y).fold(f64::NEG_INFINITY, f64::max).ceil().clamp(0.0, max) as usize;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
            let w0 = ((s[2].x - s[1].x) * (py - s[1].y) - (s[2].y - s[1].y) * (px - s[1].x)) / area;
            let w1 = ((s[0].x - s[2].x) * (py - s[2].y) - (s[0].y - s[2].y) * (px - s[2].x)) / area;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                continue;
            }
            let z = w0 * s[0].z + w1 * s[1].z + w2 * s[2].z;
            let i = y * big + x;
            if let (Some(dz), Some(c)) = (depth.get_mut(i), color.get_mut(i))
                && z < *dz
            {
                *dz = z;
                *c = rgba;
            }
        }
    }
}

fn draw_line(a: DVec3, b: DVec3, big: usize, depth: &[f64], color: &mut [[f32; 4]], bias: f64) {
    let len = (b.x - a.x).abs().max((b.y - a.y).abs());
    if !len.is_finite() {
        return;
    }
    let steps = (len.ceil() as usize).clamp(1, big * 4);
    for i in 0..=steps {
        let t = i as f64 / steps as f64;
        let p = a.lerp(b, t);
        if p.x < 0.0 || p.y < 0.0 {
            continue;
        }
        let (x, y) = (p.x as usize, p.y as usize);
        if x >= big || y >= big {
            continue;
        }
        let idx = y * big + x;
        let (Some(&dz), Some(c)) = (depth.get(idx), color.get_mut(idx)) else { continue };
        if c[3] > 0.0 && p.z <= dz + bias {
            for v in c.iter_mut().take(3) {
                *v *= 0.45;
            }
        }
    }
}

fn downsample(color: &[[f32; 4]], big: usize, size: usize) -> Thumbnail {
    let mut rgba = vec![0u8; size * size * 4];
    let n = (SS * SS) as f32;
    for y in 0..size {
        for x in 0..size {
            let mut acc = [0f32; 4];
            for dy in 0..SS {
                for dx in 0..SS {
                    let Some(c) = color.get((y * SS + dy) * big + x * SS + dx) else { continue };
                    for k in 0..3 {
                        acc[k] += c[k] * c[3];
                    }
                    acc[3] += c[3];
                }
            }
            let alpha = acc[3] / n;
            let o = (y * size + x) * 4;
            if let Some(px) = rgba.get_mut(o..o + 4)
                && acc[3] > 0.0
            {
                for k in 0..3 {
                    px[k] = (acc[k] / acc[3] * 255.0).round().clamp(0.0, 255.0) as u8;
                }
                px[3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    Thumbnail { size, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX: &str = r#"solid
{
	"id" "2"
	side { "id" "1" "plane" "(0 64 64) (64 64 64) (64 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" }
	side { "id" "2" "plane" "(0 0 0) (64 0 0) (64 64 0)" "material" "TOOLS/TOOLSNODRAW" }
	side { "id" "3" "plane" "(0 64 64) (0 0 64) (0 0 0)" "material" "DEV/DEV_MEASUREGENERIC01B" }
	side { "id" "4" "plane" "(64 64 0) (64 0 0) (64 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" }
	side { "id" "5" "plane" "(64 64 64) (0 64 64) (0 64 0)" "material" "DEV/DEV_MEASUREGENERIC01B" }
	side { "id" "6" "plane" "(64 0 0) (0 0 0) (0 0 64)" "material" "DEV/DEV_MEASUREGENERIC01B" }
}
entity { "classname" "light" "origin" "32 32 128" editor { "color" "220 30 220" } }
"#;

    #[test]
    fn draws_brushes_and_entities() {
        let t = render_text(BOX, 64).expect("thumbnail");
        assert_eq!(t.rgba.len(), 64 * 64 * 4);
        let opaque = t.rgba.chunks(4).filter(|p| p[3] == 255).count();
        let clear = t.rgba.chunks(4).filter(|p| p[3] == 0).count();
        assert!(opaque > 300, "{opaque}");
        assert!(clear > 300, "transparent background: {clear}");
        assert!(t.rgba.chunks(4).any(|p| p[3] > 0 && p[0] > 120 && p[1] < 60 && p[2] > 120));
        assert_eq!(render_text(BOX, 64), Some(t));
    }

    #[test]
    fn nothing_to_draw_and_junk() {
        assert!(render_text("", 64).is_none());
        assert!(render_text("versioninfo { }", 64).is_none());
        assert!(render_text("{{{{", 64).is_none());
        assert!(render_text("entity { \"classname\" \"x\" \"origin\" \"nan 0 0\" }", 64).is_none());
        let far = "entity { \"classname\" \"x\" \"origin\" \"1e300 0 0\" } entity { \"classname\" \"x\" \"origin\" \"0 0 0\" }";
        let _ = render_text(far, 32);
        assert_eq!(material_color("tools/toolshint"), None);
        assert_eq!(material_color("TOOLS\\TOOLSTRIGGER"), Some([0.92, 0.55, 0.18]));
        assert_eq!(material_color("a"), material_color("A"));
    }
}
