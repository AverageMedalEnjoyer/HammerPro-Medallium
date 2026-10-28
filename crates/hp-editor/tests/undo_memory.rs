#![allow(unsafe_code, clippy::unwrap_used, clippy::expect_used)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};
use std::time::Duration;

use glam::DVec3;
use hp_editor::{Document, FaceEdit, FaceRef, ObjRef};
use hp_vmf::{Entity, Map, Side, Solid, TexAxis};

struct Counting;

static LIVE_BYTES: AtomicIsize = AtomicIsize::new(0);
static LIVE_BLOCKS: AtomicIsize = AtomicIsize::new(0);

fn signed(n: usize) -> isize {
    isize::try_from(n).unwrap_or(isize::MAX)
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(signed(layout.size()), Relaxed);
        LIVE_BLOCKS.fetch_add(1, Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        LIVE_BYTES.fetch_add(signed(layout.size()), Relaxed);
        LIVE_BLOCKS.fetch_add(1, Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(signed(layout.size()), Relaxed);
        LIVE_BLOCKS.fetch_sub(1, Relaxed);
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE_BYTES.fetch_add(signed(new_size) - signed(layout.size()), Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn settled() -> (isize, isize) {
    let read = || (LIVE_BYTES.load(Relaxed), LIVE_BLOCKS.load(Relaxed));
    let mut last = read();
    let mut stable = 0;
    for _ in 0..500 {
        std::thread::sleep(Duration::from_millis(10));
        let now = read();
        if now == last {
            stable += 1;
            if stable >= 10 {
                break;
            }
        } else {
            stable = 0;
            last = now;
        }
    }
    last
}

fn freed_by_clearing(doc: &mut Document) -> f64 {
    let (bytes, blocks) = settled();
    doc.clear_history();
    let (bytes_after, blocks_after) = settled();
    ((bytes - bytes_after) + 16 * (blocks - blocks_after)) as f64
}

fn ratio(doc: &mut Document, what: &str) -> f64 {
    let estimate = doc.history_bytes() as f64;
    let real = freed_by_clearing(doc);
    assert!(real > 0.0, "{what}: the history held nothing");
    let r = estimate / real;
    eprintln!("{what}: estimate {:.2} MB, real {:.2} MB, ratio {r:.3}", estimate / 1e6, real / 1e6);
    r
}

fn cube(min: DVec3, size: f64, material: &str) -> Vec<Side> {
    let max = min + DVec3::splat(size);
    let (x0, y0, z0) = (min.x, min.y, min.z);
    let (x1, y1, z1) = (max.x, max.y, max.z);
    let v = DVec3::new;
    let faces = [
        ([v(x0, y1, z1), v(x1, y1, z1), v(x1, y0, z1)], DVec3::X, DVec3::NEG_Y),
        ([v(x0, y0, z0), v(x1, y0, z0), v(x1, y1, z0)], DVec3::X, DVec3::NEG_Y),
        ([v(x1, y1, z1), v(x0, y1, z1), v(x0, y1, z0)], DVec3::X, DVec3::NEG_Z),
        ([v(x1, y0, z0), v(x0, y0, z0), v(x0, y0, z1)], DVec3::X, DVec3::NEG_Z),
        ([v(x1, y1, z0), v(x1, y0, z0), v(x1, y0, z1)], DVec3::Y, DVec3::NEG_Z),
        ([v(x0, y1, z1), v(x0, y0, z1), v(x0, y0, z0)], DVec3::Y, DVec3::NEG_Z),
    ];
    faces
        .into_iter()
        .map(|(plane, u, w)| Side {
            plane,
            material: material.to_owned(),
            uaxis: TexAxis { axis: u, shift: 0.0, scale: 0.25 },
            vaxis: TexAxis { axis: w, shift: 0.0, scale: 0.25 },
            ..Side::default()
        })
        .collect()
}

fn sample_map() -> Map {
    let mut map = Map::new_empty();
    let mut id = 1;
    let mut next = || {
        id += 1;
        id
    };
    for i in 0..3000 {
        let min = DVec3::new(f64::from(i % 60) * 64.0, f64::from(i / 60) * 64.0, 0.0);
        let mut sides = cube(min, 48.0, "CONCRETE/CONCRETEFLOOR_STAINED001A");
        for s in &mut sides {
            s.id = next();
        }
        map.world.solids.push(Solid { id: next(), sides, ..Solid::default() });
    }
    for i in 0..300 {
        let mut e = Entity { id: next(), classname: "prop_dynamic".into(), ..Entity::default() };
        e.set("origin", format!("{} 8 0", i * 16));
        e.set("angles", "0 90 0");
        e.set("model", "models/props_c17/oildrum001.mdl");
        e.set("targetname", format!("drum_{i}"));
        e.connections.push(hp_vmf::Connection::parse("OnUser1", &format!("drum_{i},Kill,,0,-1")));
        map.entities.push(e);
    }
    hp_vmf::parse(&hp_vmf::write(&map)).expect("parse")
}

#[test]
fn history_budget_matches_real_memory() {
    let mut doc = Document::from_map(sample_map());
    let disp_faces: Vec<FaceRef> = doc.scene().solids().keys().take(40).map(|id| FaceRef::new(*id, 0)).collect();
    doc.create_displacements(&disp_faces, 3).expect("displacements");
    doc.clear_history();
    let _ = doc.take_changes();
    let all: Vec<ObjRef> = doc
        .scene()
        .solids()
        .keys()
        .map(|id| ObjRef::Solid(*id))
        .chain(doc.scene().entities().keys().map(|id| ObjRef::Entity(*id)))
        .collect();

    doc.translate(&all, DVec3::new(16.0, 0.0, 0.0)).expect("move");
    let r = ratio(&mut doc, "move");
    assert!((0.9..=1.1).contains(&r), "move: estimate / real = {r}");

    doc.translate(&all, DVec3::new(16.0, 0.0, 0.0)).expect("move");
    assert!(doc.undo());
    let r = ratio(&mut doc, "undone move");
    assert!((0.9..=1.1).contains(&r), "undone move: estimate / real = {r}");

    for _ in 0..3 {
        doc.translate(&all, DVec3::new(0.0, 16.0, 0.0)).expect("move");
    }
    let r = ratio(&mut doc, "three moves");
    assert!((0.9..=1.1).contains(&r), "three moves: estimate / real = {r}");

    let faces: Vec<FaceRef> = doc.scene().solids().keys().map(|id| FaceRef::new(*id, 1)).collect();
    let edit = FaceEdit { material: Some("BRICK/BRICKWALL001A".into()), ..FaceEdit::default() };
    doc.edit_faces(&faces, &edit).expect("texture");
    let r = ratio(&mut doc, "texture edit");
    assert!((0.95..=1.6).contains(&r), "texture edit: estimate / real = {r}");

    let ents: Vec<u32> = all.iter().filter(|o| o.is_entity()).map(|o| o.id()).collect();
    doc.set_entity_key(&ents, "skin", "1").expect("keys");
    let r = ratio(&mut doc, "entity keys");
    assert!((0.9..=1.2).contains(&r), "entity keys: estimate / real = {r}");
}
