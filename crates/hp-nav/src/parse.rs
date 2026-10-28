use crate::{HidingSpot, MAX_VERSION, NAV_MAGIC, NavArea, NavError, NavLadder, NavMesh, Result};

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn left(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).filter(|&e| e <= self.data.len()).ok_or_else(|| truncated(self.pos))?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }

    fn skip(&mut self, n: usize) -> Result<()> {
        self.take(n).map(|_| ())
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn f32(&mut self) -> Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }

    fn vec3(&mut self) -> Result<[f32; 3]> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }

    fn count(&mut self, min_size: usize, what: &str) -> Result<usize> {
        let n = self.u32()? as usize;
        self.check_count(n, min_size, what)
    }

    fn check_count(&self, n: usize, min_size: usize, what: &str) -> Result<usize> {
        if n.saturating_mul(min_size.max(1)) > self.left() {
            return Err(NavError::Corrupt(format!("{n} {what} do not fit in the file (offset {})", self.pos)));
        }
        Ok(n)
    }
}

fn truncated(at: usize) -> NavError {
    NavError::Corrupt(format!("unexpected end of file at offset {at}"))
}

struct Head {
    version: u32,
    subversion: u32,
    bsp_size: Option<u32>,
    analyzed: bool,
    places: Vec<String>,
    has_unnamed_areas: bool,
}

fn read_head(r: &mut Reader<'_>) -> Result<Head> {
    if r.data.len() < 4 || r.u32()? != NAV_MAGIC {
        return Err(NavError::NotNav);
    }
    let version = r.u32()?;
    if version == 0 || version > MAX_VERSION {
        return Err(NavError::UnsupportedVersion(version));
    }
    let subversion = if version >= 10 { r.u32()? } else { 0 };
    let bsp_size = if version >= 4 { Some(r.u32()?) } else { None };
    let analyzed = if version >= 14 { r.u8()? != 0 } else { false };
    let mut places = Vec::new();
    let mut has_unnamed_areas = false;
    if version >= 5 {
        let n = usize::from(r.u16()?);
        let n = r.check_count(n, 2, "places")?;
        places.reserve(n);
        for _ in 0..n {
            let len = usize::from(r.u16()?);
            let raw = r.take(len)?;
            let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
            places.push(String::from_utf8_lossy(&raw[..end]).into_owned());
        }
        if version > 11 {
            has_unnamed_areas = r.u8()? != 0;
        }
    }
    Ok(Head { version, subversion, bsp_size, analyzed, places, has_unnamed_areas })
}

fn min_area_size(version: u32) -> usize {
    let mut n = 4 + 1 + 24 + 8 + 16 + 1 + 4;
    if version >= 5 {
        n += 2;
    }
    if version >= 7 {
        n += 8;
    }
    if version >= 8 {
        n += 8;
    }
    if version >= 11 {
        n += 16;
    }
    if version >= 16 {
        n += 8;
    }
    n
}

fn read_ids(r: &mut Reader<'_>, what: &str) -> Result<Vec<u32>> {
    let n = r.count(4, what)?;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        out.push(r.u32()?);
    }
    Ok(out)
}

fn read_area(r: &mut Reader<'_>, version: u32, custom: usize) -> Result<NavArea> {
    let mut a = NavArea { id: r.u32()?, ..NavArea::default() };
    a.flags = if version <= 8 {
        u32::from(r.u8()?)
    } else if version < 13 {
        u32::from(r.u16()?)
    } else {
        r.u32()?
    };
    a.nw = r.vec3()?;
    a.se = r.vec3()?;
    a.ne_z = r.f32()?;
    a.sw_z = r.f32()?;
    for d in 0..4 {
        let mut ids = read_ids(r, "connections")?;
        ids.retain(|&id| id != a.id);
        a.connections[d] = ids;
    }
    let spots = usize::from(r.u8()?);
    if version == 1 {
        r.skip(spots * 12)?;
    } else {
        let spots = r.check_count(spots, 17, "hiding spots")?;
        a.hiding_spots.reserve(spots);
        for _ in 0..spots {
            a.hiding_spots.push(HidingSpot { id: r.u32()?, pos: r.vec3()?, flags: r.u8()? });
        }
    }
    if version < 15 {
        let n = usize::from(r.u8()?);
        r.skip(n * 14)?;
    }
    let encounters = r.u32()?;
    a.encounter_paths = encounters;
    if version < 3 {
        let n = r.check_count(encounters as usize, 33, "encounter paths")?;
        for _ in 0..n {
            r.skip(4 + 4 + 12 + 12)?;
            let spots = usize::from(r.u8()?);
            r.skip(spots * 16)?;
        }
        return Ok(a);
    }
    let n = r.check_count(encounters as usize, 11, "encounter paths")?;
    for _ in 0..n {
        r.skip(4 + 1 + 4 + 1)?;
        let spots = usize::from(r.u8()?);
        r.skip(spots * 5)?;
    }
    if version < 5 {
        return Ok(a);
    }
    a.place = r.u16()?;
    if version < 7 {
        return Ok(a);
    }
    a.ladders_up = read_ids(r, "ladder connections")?;
    a.ladders_down = read_ids(r, "ladder connections")?;
    if version < 8 {
        return Ok(a);
    }
    r.skip(8)?;
    if version < 11 {
        return Ok(a);
    }
    a.light = Some([r.f32()?, r.f32()?, r.f32()?, r.f32()?]);
    if version < 16 {
        return Ok(a);
    }
    let visible = r.count(5, "visible areas")?;
    r.skip(visible * 5)?;
    a.visible_areas = visible as u32;
    r.skip(4)?;
    r.skip(custom)?;
    Ok(a)
}

fn read_ladder(r: &mut Reader<'_>, version: u32) -> Result<NavLadder> {
    let mut l = NavLadder {
        id: r.u32()?,
        width: r.f32()?,
        top: r.vec3()?,
        bottom: r.vec3()?,
        length: r.f32()?,
        dir: r.u32()?,
        ..NavLadder::default()
    };
    if version == 6 {
        r.skip(1)?;
    }
    for slot in &mut l.top_areas {
        *slot = r.u32()?;
    }
    l.bottom_area = r.u32()?;
    Ok(l)
}

fn read_body(data: &[u8], start: usize, version: u32, custom: usize) -> Result<(Vec<NavArea>, Vec<NavLadder>, usize)> {
    let mut r = Reader { data, pos: start };
    let n = r.count(min_area_size(version) + custom, "areas")?;
    let mut areas = Vec::with_capacity(n);
    for _ in 0..n {
        areas.push(read_area(&mut r, version, custom)?);
    }
    let mut ladders = Vec::new();
    if version >= 6 && r.left() > 0 {
        let n = r.count(60, "ladders")?;
        ladders.reserve(n);
        for _ in 0..n {
            ladders.push(read_ladder(&mut r, version)?);
        }
    }
    Ok((areas, ladders, r.left()))
}

const CUSTOM_SIZES: [usize; 5] = [0, 4, 8, 12, 16];

pub fn parse(data: &[u8]) -> Result<NavMesh> {
    let mut r = Reader::new(data);
    let head = read_head(&mut r)?;
    let start = r.pos;
    let mut first_error = None;
    let mut best: Option<(Vec<NavArea>, Vec<NavLadder>, usize, usize)> = None;
    for custom in CUSTOM_SIZES {
        if custom > 0 && head.subversion == 0 && first_error.is_none() {
            break;
        }
        match read_body(data, start, head.version, custom) {
            Ok((areas, ladders, left)) => {
                let exact = left == 0;
                if best.as_ref().is_none_or(|b| left < b.2) {
                    best = Some((areas, ladders, left, custom));
                }
                if exact {
                    break;
                }
            }
            Err(e) => {
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }
    let Some((areas, ladders, trailing_bytes, area_custom_bytes)) = best else {
        return Err(first_error.unwrap_or_else(|| truncated(start)));
    };
    Ok(NavMesh {
        version: head.version,
        subversion: head.subversion,
        bsp_size: head.bsp_size,
        analyzed: head.analyzed,
        places: head.places,
        has_unnamed_areas: head.has_unnamed_areas,
        areas,
        ladders,
        area_custom_bytes,
        trailing_bytes,
    })
}
