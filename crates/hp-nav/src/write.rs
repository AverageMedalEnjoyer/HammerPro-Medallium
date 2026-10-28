use crate::{NAV_MAGIC, NavMesh};

pub const WRITE_VERSION: u32 = 16;

struct Out(Vec<u8>);

impl Out {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn vec3(&mut self, v: [f32; 3]) {
        for c in v {
            self.f32(c);
        }
    }
    fn ids(&mut self, ids: &[u32]) {
        self.u32(ids.len() as u32);
        for id in ids {
            self.u32(*id);
        }
    }
}

pub fn write(mesh: &NavMesh) -> Vec<u8> {
    let mut o = Out(Vec::with_capacity(64 + mesh.areas.len() * 96));
    o.u32(NAV_MAGIC);
    o.u32(WRITE_VERSION);
    o.u32(0);
    o.u32(mesh.bsp_size.unwrap_or(0));
    o.u8(u8::from(mesh.analyzed));
    let places: Vec<&String> = mesh.places.iter().take(usize::from(u16::MAX)).collect();
    o.u16(places.len() as u16);
    for p in places {
        let mut bytes: Vec<u8> = p.as_bytes().iter().copied().take(255).collect();
        bytes.push(0);
        o.u16(bytes.len() as u16);
        o.0.extend_from_slice(&bytes);
    }
    o.u8(u8::from(mesh.has_unnamed_areas));
    o.u32(mesh.areas.len() as u32);
    for a in &mesh.areas {
        o.u32(a.id);
        o.u32(a.flags);
        o.vec3(a.nw);
        o.vec3(a.se);
        o.f32(a.ne_z);
        o.f32(a.sw_z);
        for d in 0..4 {
            o.ids(&a.connections[d]);
        }
        o.u8(0);
        o.u32(0);
        o.u16(a.place);
        o.ids(&a.ladders_up);
        o.ids(&a.ladders_down);
        o.f32(0.0);
        o.f32(0.0);
        for l in a.light.unwrap_or([1.0; 4]) {
            o.f32(l);
        }
        o.u32(0);
        o.u32(0);
    }
    o.u32(0);
    o.0
}
