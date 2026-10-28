use super::*;

#[derive(Default)]
struct Writer {
    out: Vec<u8>,
}

impl Writer {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.out.push(v);
        self
    }
    fn u16(&mut self, v: u16) -> &mut Self {
        self.out.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn u32(&mut self, v: u32) -> &mut Self {
        self.out.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn f32(&mut self, v: f32) -> &mut Self {
        self.out.extend_from_slice(&v.to_le_bytes());
        self
    }
    fn vec3(&mut self, v: [f32; 3]) -> &mut Self {
        self.f32(v[0]).f32(v[1]).f32(v[2])
    }
}

fn area16(w: &mut Writer, id: u32, flags: u32, x: f32, links: [&[u32]; 4], custom: usize) {
    w.u32(id).u32(flags).vec3([x, 0.0, 0.0]).vec3([x + 50.0, 50.0, 0.0]).f32(0.0).f32(0.0);
    for d in links {
        w.u32(d.len() as u32);
        for id in d {
            w.u32(*id);
        }
    }
    w.u8(1).u32(7).vec3([x, 1.0, 2.0]).u8(1);
    w.u32(0).u16(1);
    w.u32(0).u32(0);
    w.f32(0.0).f32(0.0);
    for _ in 0..4 {
        w.f32(1.0);
    }
    w.u32(1).u32(id).u8(2).u32(0);
    for _ in 0..custom {
        w.u8(0xAB);
    }
}

fn file16(subversion: u32, custom: usize) -> Vec<u8> {
    let mut w = Writer::default();
    w.u32(NAV_MAGIC).u32(16).u32(subversion).u32(123_456).u8(1);
    w.u16(1).u16(6);
    w.out.extend_from_slice(b"Lobby\0");
    w.u8(0);
    w.u32(3);
    area16(&mut w, 1, attr::CROUCH, 0.0, [&[], &[2], &[], &[]], custom);
    area16(&mut w, 2, attr::JUMP | attr::CROUCH, 50.0, [&[], &[3], &[], &[1, 2]], custom);
    area16(&mut w, 3, 0, 100.0, [&[99], &[], &[], &[]], custom);
    w.u32(1).u32(5).f32(20.0).vec3([0.0, 0.0, 100.0]).vec3([0.0, 0.0, 0.0]).f32(100.0).u32(2);
    w.u32(1).u32(0).u32(0).u32(0).u32(3);
    w.out
}

#[test]
fn version_16_reads_every_field() {
    let mesh = parse(&file16(0, 0)).expect("parses");
    assert_eq!((mesh.version, mesh.subversion, mesh.bsp_size, mesh.analyzed), (16, 0, Some(123_456), true));
    assert_eq!(mesh.places, vec!["Lobby".to_owned()]);
    assert_eq!(mesh.areas.len(), 3);
    assert_eq!(mesh.trailing_bytes, 0);
    let a = &mesh.areas[1];
    assert_eq!(a.id, 2);
    assert_eq!(a.flags, attr::JUMP | attr::CROUCH);
    assert_eq!(a.connections[Dir::West as usize], vec![1]);
    assert_eq!(a.hiding_spots.len(), 1);
    assert_eq!(mesh.place_name(a), Some("Lobby"));
    assert_eq!(a.visible_areas, 1);
    assert_eq!(a.corners()[2], [100.0, 50.0, 0.0]);
    assert_eq!(a.center(), [75.0, 25.0, 0.0]);
    assert_eq!(a.edge_center(Dir::East), [100.0, 25.0, 0.0]);
    assert_eq!(mesh.ladders.len(), 1);
    assert_eq!(mesh.ladders[0].top_areas, [1, 0, 0, 0]);
    assert_eq!(mesh.ladders[0].bottom_area, 3);
}

#[test]
fn statistics_find_one_way_and_missing_links() {
    let mesh = parse(&file16(0, 0)).expect("parses");
    let s = mesh.stats();
    assert_eq!(s.areas, 3);
    assert_eq!(s.connections, 4);
    assert_eq!(s.two_way_pairs, 1);
    assert_eq!(s.one_way, 1);
    assert_eq!(s.dangling, 1);
    assert_eq!(s.unconnected_areas, 0);
    assert_eq!(s.components, 1);
    assert_eq!(s.isolated_areas, 0);
    assert_eq!(s.attributes, vec![("crouch", 2), ("jump", 1)]);
    assert_eq!(s.walkable_area, 3.0 * 2500.0);
    assert_eq!(s.hiding_spots, 3);
}

#[test]
fn isolated_areas_are_outside_the_largest_group() {
    let mut mesh = parse(&file16(0, 0)).expect("parses");
    mesh.ladders.clear();
    mesh.areas[1].connections[Dir::East as usize].clear();
    let (groups, count) = area_components(&mesh);
    assert_eq!(count, 2);
    assert_eq!(groups, vec![0, 0, 1]);
    let s = mesh.stats();
    assert_eq!(s.isolated_areas, 1);
    assert_eq!(s.unconnected_areas, 1);
}

#[test]
fn game_data_after_each_area_is_skipped() {
    let mesh = parse(&file16(2, 4)).expect("parses with 4 bytes of game data per area");
    assert_eq!(mesh.area_custom_bytes, 4);
    assert_eq!(mesh.areas.len(), 3);
    assert_eq!(mesh.ladders.len(), 1);
    assert_eq!(mesh.trailing_bytes, 0);
}

#[test]
fn old_versions_read() {
    let mut w = Writer::default();
    w.u32(NAV_MAGIC).u32(5).u32(1000);
    w.u16(0);
    w.u32(1);
    w.u32(4).u8(attr::JUMP as u8).vec3([0.0; 3]).vec3([10.0, 10.0, 0.0]).f32(0.0).f32(0.0);
    for _ in 0..4 {
        w.u32(0);
    }
    w.u8(0);
    w.u8(0);
    w.u32(0);
    w.u16(0);
    let mesh = parse(&w.out).expect("parses");
    assert_eq!(mesh.version, 5);
    assert_eq!(mesh.bsp_size, Some(1000));
    assert_eq!(mesh.areas[0].flags, attr::JUMP);
    assert!(mesh.ladders.is_empty());
}

#[test]
fn hostile_input_is_refused_without_panic() {
    assert!(matches!(parse(b""), Err(NavError::NotNav)));
    assert!(matches!(parse(&[0xCE, 0xFA, 0xED, 0xFE]), Err(NavError::Corrupt(_))));
    let mut w = Writer::default();
    w.u32(NAV_MAGIC).u32(99);
    assert!(matches!(parse(&w.out), Err(NavError::UnsupportedVersion(99))));
    let mut w = Writer::default();
    w.u32(NAV_MAGIC).u32(16).u32(0).u32(0).u8(0).u16(0).u8(0).u32(u32::MAX);
    assert!(matches!(parse(&w.out), Err(NavError::Corrupt(_))));
    let full = file16(0, 0);
    for n in 0..full.len() {
        let _ = parse(&full[..n]);
    }
    let mut seed = 0x1234_5678u32;
    for _ in 0..200 {
        let mut data = full.clone();
        for _ in 0..8 {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let i = (seed as usize) % data.len();
            data[i] = (seed >> 24) as u8;
        }
        if let Ok(mesh) = parse(&data) {
            let _ = mesh.stats();
            let _ = mesh.bounds();
        }
    }
}

#[test]
fn real_garrys_mod_meshes() {
    let roots = [
        "D:/SteamLibrary/steamapps/common/GarrysMod/garrysmod/maps",
        "C:/Program Files (x86)/Steam/steamapps/common/GarrysMod/garrysmod/maps",
    ];
    let Some(dir) = roots.iter().map(std::path::Path::new).find(|p| p.is_dir()) else { return };
    let Ok(read) = std::fs::read_dir(dir) else { return };
    for entry in read.flatten() {
        let path = entry.path();
        if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("nav")) {
            continue;
        }
        let mesh = load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert_eq!(mesh.trailing_bytes, 0, "{}: the whole file is read", path.display());
        let s = mesh.stats();
        eprintln!("{}: v{} {:?}", path.display(), mesh.version, s);
        assert!(s.areas > 0);
    }
}

#[test]
fn the_main_part_is_where_players_spawn() {
    let mut mesh = parse(&file16(0, 0)).expect("parses");
    mesh.ladders.clear();
    mesh.areas[1].connections[Dir::East as usize].clear();
    let (groups, _) = area_components(&mesh);
    assert_eq!(groups, vec![0, 0, 1]);
    assert_eq!(mesh.area_at([120.0, 20.0, 40.0]), Some(2));
    assert_eq!(mesh.area_at([120.0, 20.0, 400.0]), None, "too high above");
    assert_eq!(mesh.area_at([1000.0, 20.0, 0.0]), None);
    let (groups, _) = mesh.components_from(&[[120.0, 20.0, 10.0]]);
    assert_eq!(groups, vec![1, 1, 0]);
    let s = mesh.stats_from(&[[120.0, 20.0, 10.0]]);
    assert_eq!(s.isolated_areas, 2);
    assert_eq!(mesh.stats_from(&[[9999.0, 0.0, 0.0]]).isolated_areas, 1);
}
