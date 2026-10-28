use hp_vmf::{LineEnding, NumberStyle, Side, TextEncoding, parse_bytes, parse_bytes_with_report, write, write_bytes};

fn side(id: u32, plane: &str, u: &str, v: &str, extra: &str) -> String {
    format!(
        "\t\tside\n\t\t{{\n\t\t\t\"id\" \"{id}\"\n\t\t\t\"plane\" \"{plane}\"\n\t\t\t\"material\" \"DEV/DEV_MEASUREGENERIC01\"\n\
\t\t\t\"uaxis\" \"{u}\"\n\t\t\t\"vaxis\" \"{v}\"\n\t\t\t\"rotation\" \"0\"\n\t\t\t\"lightmapscale\" \"16\"\n\
\t\t\t\"smoothing_groups\" \"0\"\n{extra}\t\t}}\n"
    )
}

fn rows(name: &str, vals: &[&str]) -> String {
    let t = "\t\t\t\t";
    let mut s = format!("{t}{name}\n{t}{{\n");
    for (i, r) in vals.iter().enumerate() {
        s.push_str(&format!("{t}\t\"row{i}\" \"{r}\"\n"));
    }
    s.push_str(&format!("{t}}}\n"));
    s
}

fn disp() -> String {
    let t = "\t\t\t\t";
    let n = "0 0 1 0 0 1 -1.19209e-007 0 1 0 0 1 0 0 1";
    let mut d = format!(
        "\t\t\tdispinfo\n\t\t\t{{\n{t}\"power\" \"2\"\n{t}\"startposition\" \"[0 0 64]\"\n{t}\"flags\" \"0\"\n\
{t}\"elevation\" \"0\"\n{t}\"subdiv\" \"0\"\n"
    );
    d.push_str(&rows("normals", &[n; 5]));
    d.push_str(&rows(
        "distances",
        &["0 1.5 2.25 0 0", "0 0 0 0 0", "0 0 12.3457 0 0", "0 0 0 0 0", "0 0 0 0 1.5e-005"],
    ));
    d.push_str(&rows("offsets", &["0 0 0 0 0 0 0 0 0 0 0 0 0 0 0"; 5]));
    d.push_str(&rows("offset_normals", &["0 0 1 0 0 1 0 0 1 0 0 1 0 0 1"; 5]));
    d.push_str(&rows("alphas", &["0 0 0 0 0", "255 255 128 0 0", "0 0 0 0 0", "0 0 0 0 0", "0 0 0 0 0"]));
    d.push_str(&rows("triangle_tags", &["9 9 9 9 9 9 9 9", "9 9 1 1 9 9 9 9", "9 9 9 9 9 9 9 9", "9 9 9 9 9 9 9 9"]));
    d.push_str(&format!("{t}allowed_verts\n{t}{{\n{t}\t\"10\" \"-1 -1 -1 -1 -1 -1 -1 -1 -1 -1\"\n{t}}}\n\t\t\t}}\n"));
    d
}

fn solid(id: u32, first_side: u32, top_extra: &str, bad_plane: Option<&str>) -> String {
    let (x0, y0, z0, x1, y1, z1) = (0, 0, 0, 64, 64, 64);
    let planes = [
        format!("({x0} {y1} {z1}) ({x1} {y1} {z1}) ({x1} {y0} {z1})"),
        format!("({x0} {y0} {z0}) ({x1} {y0} {z0}) ({x1} {y1} {z0})"),
        format!("({x0} {y1} {z1}) ({x0} {y0} {z1}) ({x0} {y0} {z0})"),
        format!("({x1} {y1} {z0}) ({x1} {y0} {z0}) ({x1} {y0} {z1})"),
        format!("({x1} {y1} {z1}) ({x0} {y1} {z1}) ({x0} {y1} {z0})"),
        format!("({x1} {y0} {z0}) ({x0} {y0} {z0}) ({x0} {y0} {z1})"),
    ];
    let uv = [
        ("[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
        ("[1 0 0 0] 0.25", "[0 -1 0 0] 0.25"),
        ("[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
        ("[0 1 0 0] 0.25", "[0 0 -1 0] 0.25"),
        ("[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
        ("[1 0 0 0] 0.25", "[0 0 -1 0] 0.25"),
    ];
    let mut s = format!("\tsolid\n\t{{\n\t\t\"id\" \"{id}\"\n");
    for (k, p) in planes.iter().enumerate() {
        let plane = match (k, bad_plane) {
            (1, Some(bad)) => bad,
            _ => p.as_str(),
        };
        let extra = if k == 0 { top_extra } else { "" };
        s.push_str(&side(first_side + k as u32, plane, uv[k].0, uv[k].1, extra));
    }
    s.push_str("\t\teditor\n\t\t{\n\t\t\t\"color\" \"0 180 255\"\n\t\t\t\"visgroupshown\" \"1\"\n\t\t\t\"visgroupautoshown\" \"1\"\n\t\t}\n\t}\n");
    s
}

fn entity(id: u32, class: &str, props: &[(&str, &str)], conns: &[(&str, &str)]) -> String {
    let mut s = format!("entity\n{{\n\t\"id\" \"{id}\"\n\t\"classname\" \"{class}\"\n");
    for (k, v) in props {
        s.push_str(&format!("\t\"{k}\" \"{v}\"\n"));
    }
    if !conns.is_empty() {
        s.push_str("\tconnections\n\t{\n");
        for (k, v) in conns {
            s.push_str(&format!("\t\t\"{k}\" \"{v}\"\n"));
        }
        s.push_str("\t}\n");
    }
    s.push_str("\t\"origin\" \"0 0 0\"\n\teditor\n\t{\n\t\t\"color\" \"220 30 220\"\n\t\t\"visgroupshown\" \"1\"\n\t\t\"visgroupautoshown\" \"1\"\n\t\t\"logicalpos\" \"[0 0]\"\n\t}\n}\n");
    s
}

fn hammer_map(bad_plane: Option<&str>) -> String {
    let mut s = String::from(
        "versioninfo\n{\n\t\"editorversion\" \"400\"\n\t\"editorbuild\" \"8864\"\n\t\"mapversion\" \"3\"\n\t\"formatversion\" \"100\"\n\
\t\"prefab\" \"0\"\n}\nvisgroups\n{\n}\nviewsettings\n{\n\t\"bSnapToGrid\" \"1\"\n\t\"bShowGrid\" \"1\"\n\
\t\"bShowLogicalGrid\" \"0\"\n\t\"nGridSpacing\" \"64\"\n\t\"bShow3DGrid\" \"0\"\n}\nworld\n{\n\t\"id\" \"1\"\n\
\t\"mapversion\" \"3\"\n\t\"classname\" \"worldspawn\"\n\t\"detailmaterial\" \"detail/detailsprites\"\n\
\t\"detailvbsp\" \"detail.vbsp\"\n\t\"maxpropscreenwidth\" \"-1\"\n\t\"skyname\" \"sky_day01_01\"\n",
    );
    s.push_str(&solid(2, 20, &disp(), bad_plane));
    s.push_str("}\n");
    s.push_str(&entity(
        3,
        "game_text",
        &[("targetname", "porte_\u{e9}"), ("message", "Bienvenue \u{e0} la maison \u{2019}")],
        &[],
    ));
    s.push_str(&entity(4, "game_text", &[("targetname", "porte_\u{e8}"), ("message", "D\u{e9}j\u{e0}")], &[]));
    s.push_str(&entity(
        5,
        "logic_auto",
        &[("spawnflags", "1")],
        &[("OnMapSpawn", "porte_\u{e9},Display,,0,-1"), ("OnMapSpawn", "porte_\u{e8},Display,,1,-1")],
    ));
    s.push_str(
        "cameras\n{\n\t\"activecamera\" \"-1\"\n}\ncordon\n{\n\t\"mins\" \"(-1024 -1024 -1024)\"\n\t\"maxs\" \"(1024 1024 1024)\"\n\t\"active\" \"0\"\n}\n",
    );
    s
}

#[test]
fn hammer_layout_round_trips_byte_for_byte() {
    let text = hammer_map(None);
    let map = parse_bytes(text.as_bytes()).unwrap();
    assert_eq!(map.format.numbers, NumberStyle::Exponent(3));
    assert_eq!(map.format.line_ending, LineEnding::Lf);
    let d = map.world.solids[0].sides[0].disp.as_ref().unwrap();
    assert_eq!(d.normals[2].x, -1.19209e-7);
    assert_eq!(write(&map), text);
}

#[test]
fn crlf_files_keep_crlf() {
    let text = hammer_map(None).replace('\n', "\r\n");
    let map = parse_bytes(text.as_bytes()).unwrap();
    assert_eq!(map.format.line_ending, LineEnding::CrLf);
    let (bytes, enc) = write_bytes(&map);
    assert_eq!(enc, TextEncoding::Utf8);
    assert!(bytes == text.as_bytes(), "CRLF map must be written back unchanged");
    assert!(!write(&hp_vmf::Map::new_empty()).contains('\r'));
}

#[test]
fn windows_1252_files_keep_their_bytes() {
    let text = hammer_map(None).replace('\n', "\r\n");
    let bytes = hp_kv::encode_windows_1252(&text).expect("text fits in Windows-1252");
    assert!(std::str::from_utf8(&bytes).is_err(), "fixture must not be valid UTF-8");
    let (map, report) = parse_bytes_with_report(&bytes).unwrap();
    assert_eq!(map.format.encoding, TextEncoding::Windows1252);
    assert!(report.is_clean(), "{:?}", report.warnings());
    let names: Vec<&str> = map.entities.iter().filter_map(|e| e.get("targetname")).collect();
    assert_eq!(names, ["porte_\u{e9}", "porte_\u{e8}"]);
    let targets: Vec<&str> = map.entities[2].connections.iter().map(|c| c.target.as_str()).collect();
    assert_eq!(targets, ["porte_\u{e9}", "porte_\u{e8}"]);
    let (out, enc) = write_bytes(&map);
    assert_eq!(enc, TextEncoding::Windows1252);
    assert!(out == bytes, "Windows-1252 map must be written back with the same bytes");

    let mut edited = map;
    edited.entities[0].set("message", "\u{65e5}\u{672c}");
    let (out, enc) = write_bytes(&edited);
    assert_eq!(enc, TextEncoding::Utf8);
    let back = parse_bytes(&out).unwrap();
    assert_eq!(back.entities[0].get("message"), Some("\u{65e5}\u{672c}"));
    assert_eq!(back.entities[1].get("targetname"), Some("porte_\u{e8}"));
}

#[test]
fn unreadable_plane_is_written_once_and_not_invented() {
    let bad = "(-1.#IND -1.#IND -1.#IND) (0 0 0) (1 0 0)";
    let text = hammer_map(Some(bad));
    let (map, report) = parse_bytes_with_report(text.as_bytes()).unwrap();
    let side = &map.world.solids[0].sides[1];
    assert_eq!(side.plane, Side::UNREADABLE_PLANE);
    assert_eq!(report.unreadable_values, 1);
    assert!(report.unreadable_examples[0].contains("-1.#IND"));
    assert!(!report.is_damaged());
    let out = write(&map);
    assert_eq!(out.matches("\"plane\"").count(), 6);
    assert_eq!(out, text, "the raw plane is written in place of the typed one");
}

#[test]
fn truncated_files_are_reported() {
    let text = hammer_map(None);
    let cut = text.find("(64 64 0) (64 0 0)").unwrap() + 6;
    let (map, report) = parse_bytes_with_report(&text.as_bytes()[..cut]).unwrap();
    assert!(report.is_damaged());
    assert!(report.syntax.unterminated_string);
    assert!(report.syntax.unclosed_blocks >= 3);
    assert!(map.entities.is_empty());
    let warnings = report.warnings();
    assert!(warnings.iter().any(|w| w.contains("truncated")), "{warnings:?}");

    let cut = text.find("entity\n{\n\t\"id\" \"4\"").unwrap() + 20;
    let (_, report) = parse_bytes_with_report(&text.as_bytes()[..cut]).unwrap();
    assert!(report.is_damaged());
    assert!(!report.syntax.unterminated_string);

    let (_, report) = parse_bytes_with_report(text.as_bytes()).unwrap();
    assert!(report.is_clean());
    assert!(report.warnings().is_empty());
}

#[test]
fn fragments_carry_groups_and_visgroups() {
    let text = "visgroups\n{\n\tvisgroup\n\t{\n\t\t\"name\" \"Lights\"\n\t\t\"visgroupid\" \"1\"\n\t\t\"color\" \"1 2 3\"\n\
\t\tvisgroup\n\t\t{\n\t\t\t\"name\" \"Spots\"\n\t\t\t\"visgroupid\" \"2\"\n\t\t\t\"color\" \"4 5 6\"\n\t\t}\n\t}\n}\n\
world\n{\n\t\"id\" \"1\"\n\t\"classname\" \"worldspawn\"\n\tsolid\n\t{\n\t\t\"id\" \"2\"\n\t\teditor\n\t\t{\n\
\t\t\t\"groupid\" \"20\"\n\t\t}\n\t}\n\tgroup\n\t{\n\t\t\"id\" \"20\"\n\t}\n}\n";
    let frag = hp_vmf::parse_fragment_full(text).unwrap();
    assert_eq!(frag.solids.len(), 1);
    assert_eq!(frag.groups.len(), 1);
    assert_eq!(frag.groups[0].id, 20);
    assert_eq!(frag.visgroups.len(), 1);
    assert_eq!(frag.visgroups[0].children[0].name, "Spots");
}
