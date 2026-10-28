use glam::DVec3;

use crate::{Entity, Map, Side, Solid, TexAxis, VmfError};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MapImportReport {
    pub entities: usize,
    pub brushes: usize,
    pub valve_faces: usize,
    pub quake_faces: usize,
    pub skipped_blocks: usize,
    pub bad_faces: usize,
}

struct Cursor<'a> {
    text: &'a [u8],
    pos: usize,
    line: usize,
}

fn err(line: usize, what: &str) -> VmfError {
    VmfError::Parse(format!("line {line}: {what}"))
}

impl<'a> Cursor<'a> {
    fn skip_space(&mut self) {
        while let Some(&c) = self.text.get(self.pos) {
            if c == b'\n' {
                self.line += 1;
                self.pos += 1;
            } else if c.is_ascii_whitespace() {
                self.pos += 1;
            } else if c == b'/' && self.text.get(self.pos + 1) == Some(&b'/') {
                while self.text.get(self.pos).is_some_and(|&c| c != b'\n') {
                    self.pos += 1;
                }
            } else {
                break;
            }
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        self.text.get(self.pos).copied()
    }

    fn expect(&mut self, c: u8) -> Result<(), VmfError> {
        if self.peek() == Some(c) {
            self.pos += 1;
            Ok(())
        } else {
            Err(err(self.line, &format!("'{}' expected", char::from(c))))
        }
    }

    fn quoted(&mut self) -> Result<String, VmfError> {
        self.expect(b'"')?;
        let start = self.pos;
        while let Some(&c) = self.text.get(self.pos) {
            if c == b'"' {
                let s = String::from_utf8_lossy(&self.text[start..self.pos]).into_owned();
                self.pos += 1;
                return Ok(s);
            }
            if c == b'\n' {
                self.line += 1;
            }
            self.pos += 1;
        }
        Err(err(self.line, "unterminated string"))
    }

    fn word(&mut self) -> Result<String, VmfError> {
        self.skip_space();
        let start = self.pos;
        while self.text.get(self.pos).is_some_and(|c| !c.is_ascii_whitespace()) {
            self.pos += 1;
        }
        if start == self.pos {
            return Err(err(self.line, "word expected"));
        }
        Ok(String::from_utf8_lossy(&self.text[start..self.pos]).into_owned())
    }

    fn number(&mut self) -> Result<f64, VmfError> {
        self.skip_space();
        let start = self.pos;
        while self.text.get(self.pos).is_some_and(|&c| !c.is_ascii_whitespace() && !b"()[]{}".contains(&c)) {
            self.pos += 1;
        }
        let s = std::str::from_utf8(&self.text[start..self.pos]).map_err(|_| err(self.line, "invalid number"))?;
        s.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or_else(|| err(self.line, &format!("bad number {s:?}")))
    }

    fn point(&mut self) -> Result<DVec3, VmfError> {
        self.expect(b'(')?;
        let p = DVec3::new(self.number()?, self.number()?, self.number()?);
        self.expect(b')')?;
        Ok(p)
    }

    fn axis(&mut self) -> Result<(DVec3, f64), VmfError> {
        self.expect(b'[')?;
        let a = DVec3::new(self.number()?, self.number()?, self.number()?);
        let shift = self.number()?;
        self.expect(b']')?;
        Ok((a, shift))
    }

    fn skip_block(&mut self) -> Result<(), VmfError> {
        let mut depth = 1usize;
        while depth > 0 {
            match self.peek() {
                None => return Err(err(self.line, "unclosed block")),
                Some(b'{') => depth += 1,
                Some(b'}') => depth -= 1,
                Some(b'"') => {
                    self.quoted()?;
                    continue;
                }
                Some(_) => {}
            }
            self.pos += 1;
        }
        Ok(())
    }

    fn rest_of_line_numbers(&mut self) {
        while let Some(&c) = self.text.get(self.pos) {
            if c == b'\n' {
                break;
            }
            if !c.is_ascii_whitespace() && !c.is_ascii_digit() && !b"-+.eE".contains(&c) {
                break;
            }
            self.pos += 1;
        }
    }
}

const BASE_AXES: [[DVec3; 3]; 6] = [
    [DVec3::Z, DVec3::X, DVec3::NEG_Y],
    [DVec3::NEG_Z, DVec3::X, DVec3::NEG_Y],
    [DVec3::X, DVec3::Y, DVec3::NEG_Z],
    [DVec3::NEG_X, DVec3::Y, DVec3::NEG_Z],
    [DVec3::Y, DVec3::X, DVec3::NEG_Z],
    [DVec3::NEG_Y, DVec3::X, DVec3::NEG_Z],
];

fn quake_axes(normal: DVec3, rotation: f64) -> (DVec3, DVec3) {
    let mut best = 0;
    let mut best_dot = f64::NEG_INFINITY;
    for (i, axes) in BASE_AXES.iter().enumerate() {
        let d = normal.dot(axes[0]);
        if d > best_dot + 1e-9 {
            best_dot = d;
            best = i;
        }
    }
    let (mut u, mut v) = (BASE_AXES[best][1], BASE_AXES[best][2]);
    let index = |a: DVec3| {
        if a.x != 0.0 {
            0
        } else if a.y != 0.0 {
            1
        } else {
            2
        }
    };
    let (sv, tv) = (index(u), index(v));
    let (sin, cos) = rotation.to_radians().sin_cos();
    for a in [&mut u, &mut v] {
        let (s, t) = (a[sv], a[tv]);
        a[sv] = cos * s - sin * t;
        a[tv] = sin * s + cos * t;
    }
    (u, v)
}

struct Ids {
    next: u32,
}

impl Ids {
    fn take(&mut self) -> u32 {
        let id = self.next;
        self.next = self.next.saturating_add(1);
        id
    }
}

fn face(c: &mut Cursor<'_>, ids: &mut Ids, report: &mut MapImportReport) -> Result<Option<Side>, VmfError> {
    let plane = [c.point()?, c.point()?, c.point()?];
    let material = c.word()?;
    let normal = (plane[0] - plane[1]).cross(plane[2] - plane[1]);
    let (uaxis, vaxis, rotation) = if c.peek() == Some(b'[') {
        let (u, ushift) = c.axis()?;
        let (v, vshift) = c.axis()?;
        let rotation = c.number()?;
        let (us, vs) = (c.number()?, c.number()?);
        report.valve_faces += 1;
        (TexAxis { axis: u, shift: ushift, scale: us }, TexAxis { axis: v, shift: vshift, scale: vs }, rotation)
    } else {
        let (xoff, yoff, rotation) = (c.number()?, c.number()?, c.number()?);
        let (xs, ys) = (c.number()?, c.number()?);
        let (u, v) = quake_axes(normal.normalize_or_zero(), rotation);
        report.quake_faces += 1;
        let scale = |s: f64| if s == 0.0 { 1.0 } else { s };
        (
            TexAxis { axis: u, shift: xoff, scale: scale(xs) },
            TexAxis { axis: v, shift: yoff, scale: scale(ys) },
            rotation,
        )
    };
    c.rest_of_line_numbers();
    if normal.length_squared() < 1e-12 {
        report.bad_faces += 1;
        return Ok(None);
    }
    let fix = |a: TexAxis| TexAxis { scale: if a.scale == 0.0 { 1.0 } else { a.scale }, ..a };
    Ok(Some(Side {
        id: ids.take(),
        plane,
        material,
        uaxis: fix(uaxis),
        vaxis: fix(vaxis),
        rotation,
        ..Side::default()
    }))
}

fn brush(c: &mut Cursor<'_>, ids: &mut Ids, report: &mut MapImportReport) -> Result<Option<Solid>, VmfError> {
    if c.peek().is_some_and(|b| b.is_ascii_alphabetic()) {
        c.word()?;
        c.expect(b'{')?;
        c.skip_block()?;
        c.expect(b'}')?;
        report.skipped_blocks += 1;
        return Ok(None);
    }
    let id = ids.take();
    let mut sides = Vec::new();
    loop {
        match c.peek() {
            Some(b'}') => {
                c.pos += 1;
                break;
            }
            Some(b'(') => {
                if let Some(side) = face(c, ids, report)? {
                    sides.push(side);
                }
            }
            None => return Err(err(c.line, "brush not closed")),
            Some(_) => return Err(err(c.line, "face expected")),
        }
    }
    report.brushes += 1;
    Ok((sides.len() >= 4).then(|| Solid { id, sides, ..Solid::default() }))
}

pub fn parse_map_file(bytes: &[u8]) -> Result<(Map, MapImportReport), VmfError> {
    if crate::scan::looks_binary(bytes) {
        return Err(VmfError::NotText);
    }
    let mut c = Cursor { text: bytes, pos: 0, line: 1 };
    let mut map = Map::new_empty();
    let mut report = MapImportReport::default();
    let mut ids = Ids { next: 2 };
    let mut first = true;
    while let Some(ch) = c.peek() {
        if ch != b'{' {
            return Err(err(c.line, "entity expected"));
        }
        c.pos += 1;
        let mut entity = Entity::default();
        loop {
            match c.peek() {
                Some(b'"') => {
                    let key = c.quoted()?;
                    let value = c.quoted()?;
                    if key.eq_ignore_ascii_case("classname") {
                        entity.classname = value;
                    } else {
                        entity.props.push((key, value));
                    }
                }
                Some(b'{') => {
                    c.pos += 1;
                    if let Some(solid) = brush(&mut c, &mut ids, &mut report)? {
                        entity.solids.push(solid);
                    }
                }
                Some(b'}') => {
                    c.pos += 1;
                    break;
                }
                None => return Err(err(c.line, "entity not closed")),
                Some(_) => return Err(err(c.line, "key or brush expected")),
            }
        }
        report.entities += 1;
        if first || entity.classname.eq_ignore_ascii_case("worldspawn") {
            first = false;
            entity.props.retain(|(k, _)| !k.eq_ignore_ascii_case("mapversion"));
            map.world.props.extend(entity.props);
            map.world.solids.extend(entity.solids);
        } else {
            entity.id = ids.take();
            map.entities.push(entity);
        }
    }
    if report.entities == 0 {
        return Err(VmfError::Parse("no entity in the file".to_owned()));
    }
    Ok((map, report))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALVE: &str = r#"// Game: Half-Life
// Format: Valve
// entity 0
{
"classname" "worldspawn"
"mapversion" "220"
"wad" "halflife.wad"
// brush 0
{
( -64 -64 -16 ) ( -64 -63 -16 ) ( -64 -64 -15 ) CRATE [ 0 -1 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -64 -64 -16 ) ( -64 -64 -15 ) ( -63 -64 -16 ) CRATE [ 1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( -64 -64 -16 ) ( -63 -64 -16 ) ( -64 -63 -16 ) {GRATE [ -1 0 0 8 ] [ 0 -1 0 4 ] 0 0.5 0.5
( 64 64 16 ) ( 64 65 16 ) ( 65 64 16 ) CRATE [ 1 0 0 0 ] [ 0 -1 0 0 ] 0 1 1
( 64 64 16 ) ( 65 64 16 ) ( 64 64 17 ) CRATE [ -1 0 0 0 ] [ 0 0 -1 0 ] 0 1 1
( 64 64 16 ) ( 64 64 17 ) ( 64 65 16 ) CRATE [ 0 1 0 0 ] [ 0 0 -1 0 ] 0 1 1
}
}
// entity 1
{
"classname" "info_player_start"
"origin" "0 0 40"
}
"#;

    #[test]
    fn valve_220_maps_become_vmf_maps() {
        let (map, report) = parse_map_file(VALVE.as_bytes()).expect("parses");
        assert_eq!(report.entities, 2);
        assert_eq!((report.brushes, report.valve_faces, report.quake_faces), (1, 6, 0));
        assert_eq!(map.world.solids.len(), 1);
        let solid = &map.world.solids[0];
        assert_eq!(solid.sides.len(), 6);
        assert_eq!(solid.sides[2].material, "{GRATE", "texture names may start with a brace");
        assert_eq!(solid.sides[2].uaxis.shift, 8.0);
        assert_eq!(solid.sides[2].uaxis.scale, 0.5);
        assert!(map.world.get("mapversion").is_none());
        assert_eq!(map.world.get("wad"), Some("halflife.wad"));
        assert_eq!(map.entities.len(), 1);
        assert_eq!(map.entities[0].classname, "info_player_start");
        let mut ids: Vec<u32> = solid.sides.iter().map(|s| s.id).collect();
        ids.push(solid.id);
        ids.push(map.entities[0].id);
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n);
        let bottom = &solid.sides[2].plane;
        let normal = (bottom[0] - bottom[1]).cross(bottom[2] - bottom[1]).normalize();
        assert!((normal - DVec3::NEG_Z).length() < 1e-9, "{normal:?}");
        let text = crate::write(&map);
        let back = crate::parse(&text).expect("loads back");
        assert_eq!(back.world.solids[0].sides.len(), 6);
    }

    #[test]
    fn quake_faces_get_base_axes() {
        let quake = "{\n\"classname\" \"worldspawn\"\n{\n\
( 0 0 0 ) ( 0 1 0 ) ( 1 0 0 ) floor 16 8 0 1 1\n\
( 0 0 64 ) ( 1 0 64 ) ( 0 1 64 ) ceil 0 0 90 2 2 0 0 0\n\
( 0 0 0 ) ( 0 0 1 ) ( 0 1 0 ) w 0 0 0 0 0\n\
( 64 0 0 ) ( 64 1 0 ) ( 64 0 1 ) e 0 0 0 1 1\n\
( 0 0 0 ) ( 1 0 0 ) ( 0 0 1 ) s 0 0 0 1 1\n\
( 0 64 0 ) ( 0 64 1 ) ( 1 64 0 ) n 0 0 0 1 1\n}\n}\n";
        let (map, report) = parse_map_file(quake.as_bytes()).expect("parses");
        assert_eq!(report.quake_faces, 6);
        let sides = &map.world.solids[0].sides;
        assert_eq!(sides[0].uaxis.axis, DVec3::X);
        assert_eq!(sides[0].vaxis.axis, DVec3::NEG_Y);
        assert_eq!((sides[0].uaxis.shift, sides[0].vaxis.shift), (16.0, 8.0));
        assert!((sides[1].uaxis.axis - DVec3::Y).length() < 1e-9, "{:?}", sides[1].uaxis.axis);
        assert_eq!(sides[1].uaxis.scale, 2.0);
        assert_eq!(sides[2].uaxis.scale, 1.0);
    }

    #[test]
    fn broken_and_hostile_files_are_refused_without_panicking() {
        for text in [
            "",
            "{",
            "{ \"classname\" ",
            "{ \"classname\" \"worldspawn\" { ( 0 0 0 ) ( 1 1 1 ) }",
            "{ \"classname\" \"worldspawn\" { ( 0 0 nan ) ( 0 1 0 ) ( 1 0 0 ) tex 0 0 0 1 1 } }",
            "garbage",
            "{ { { { {",
            "{ \"a\" \"b\" } }",
        ] {
            assert!(parse_map_file(text.as_bytes()).is_err(), "{text:?}");
        }
        assert!(matches!(parse_map_file(&[0, 1, 2, 3, 0, 0, 0, 0, 0, 255]), Err(VmfError::NotText)));
        let q3 = "{\n\"classname\" \"worldspawn\"\n{\npatchDef2\n{\nsome/tex\n( 3 3 0 0 0 )\n(\n( ( 0 0 0 0 0 ) )\n)\n}\n}\n}\n";
        let (map, report) = parse_map_file(q3.as_bytes()).expect("parses");
        assert_eq!(report.skipped_blocks, 1);
        assert!(map.world.solids.is_empty());
        let degenerate = "{ \"classname\" \"worldspawn\" { ( 0 0 0 ) ( 0 0 0 ) ( 0 0 0 ) t 0 0 0 1 1 } }";
        let (map, report) = parse_map_file(degenerate.as_bytes()).expect("parses");
        assert_eq!(report.bad_faces, 1);
        assert!(map.world.solids.is_empty());
    }
}
