#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParticleFile {
    pub systems: Vec<(String, String)>,
}

impl ParticleFile {
    pub fn materials(&self) -> Vec<String> {
        let mut out: Vec<String> = self.systems.iter().map(|(_, m)| m.clone()).filter(|m| !m.is_empty()).collect();
        out.sort_unstable();
        out.dedup();
        out
    }
}

#[derive(Clone, Copy)]
struct Layout {
    count_i32: bool,
    index_i32: bool,
    names_indexed: bool,
    values_indexed: bool,
}

struct Cur<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.p..self.p.checked_add(n)?)?;
        self.p += n;
        Some(s)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes([b[0], b[1]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.take(4).map(|b| i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn count(&mut self, item_min: usize) -> Option<usize> {
        let n = usize::try_from(self.i32()?).ok()?;
        (n.saturating_mul(item_min) <= self.d.len() - self.p).then_some(n)
    }
    fn cstr(&mut self) -> Option<&'a [u8]> {
        let rest = self.d.get(self.p..)?;
        let n = rest.iter().position(|&b| b == 0)?;
        self.p += n + 1;
        Some(&rest[..n])
    }
}

fn fixed_size(t: u8) -> Option<usize> {
    Some(match t {
        2 | 3 | 8 => 4,
        4 => 1,
        7 => 16,
        9 => 8,
        10 | 12 => 12,
        11 | 13 => 16,
        14 => 64,
        1 => 4,
        _ => return None,
    })
}

const TYPE_STRING: u8 = 5;
const TYPE_VOID: u8 = 6;
const TYPE_ELEMENT: u8 = 1;
const ARRAY_BASE: u8 = 14;

fn parse_with(data: &[u8], body: usize, version: u32, layout: Layout) -> Option<ParticleFile> {
    let mut c = Cur { d: data, p: body };
    let mut table: Vec<&[u8]> = Vec::new();
    if version >= 2 {
        let n = if layout.count_i32 { c.count(1)? } else { usize::from(c.u16()?) };
        table.reserve(n.min(data.len()));
        for _ in 0..n {
            table.push(c.cstr()?);
        }
    }
    let wide = layout.index_i32;
    let index = |c: &mut Cur<'_>| -> Option<usize> {
        if wide { usize::try_from(c.i32()?).ok() } else { c.u16().map(usize::from) }
    };
    let lookup = |i: usize| table.get(i).copied();
    let elements = c.count(17)?;
    let mut kinds: Vec<&[u8]> = Vec::with_capacity(elements);
    let mut names: Vec<&[u8]> = Vec::with_capacity(elements);
    for _ in 0..elements {
        let kind = if version >= 2 { lookup(index(&mut c)?)? } else { c.cstr()? };
        let name = if layout.names_indexed { lookup(index(&mut c)?)? } else { c.cstr()? };
        c.take(16)?;
        kinds.push(kind);
        names.push(name);
    }
    let mut systems = Vec::new();
    for i in 0..elements {
        let attrs = c.count(2)?;
        let mut material: Option<String> = None;
        for _ in 0..attrs {
            let name = if version >= 2 { lookup(index(&mut c)?)? } else { c.cstr()? };
            let t = c.u8()?;
            match t {
                TYPE_STRING => {
                    let v = if layout.values_indexed { lookup(index(&mut c)?)? } else { c.cstr()? };
                    if name.eq_ignore_ascii_case(b"material") {
                        material = Some(String::from_utf8_lossy(v).into_owned());
                    }
                }
                TYPE_VOID => {
                    let n = c.count(1)?;
                    c.take(n)?;
                }
                TYPE_ELEMENT => {
                    if c.i32()? == -2 {
                        c.cstr()?;
                    }
                }
                t if t > ARRAY_BASE => {
                    let base = t - ARRAY_BASE;
                    let n = c.count(1)?;
                    match base {
                        TYPE_STRING => {
                            for _ in 0..n {
                                c.cstr()?;
                            }
                        }
                        TYPE_VOID => {
                            for _ in 0..n {
                                let k = c.count(1)?;
                                c.take(k)?;
                            }
                        }
                        b => {
                            let size = fixed_size(b)?;
                            c.take(n.checked_mul(size)?)?;
                        }
                    }
                }
                t => {
                    c.take(fixed_size(t)?)?;
                }
            }
        }
        if kinds[i] == b"DmeParticleSystemDefinition" {
            let name = String::from_utf8_lossy(names[i]).into_owned();
            let material = material.map(|m| hp_vmt::normalize_material_name(&m)).unwrap_or_default();
            systems.push((name, material));
        }
    }
    (c.p == data.len()).then_some(ParticleFile { systems })
}

pub fn parse_pcf(data: &[u8]) -> Option<ParticleFile> {
    let head_end = data.iter().take(256).position(|&b| b == 0)?;
    let head = std::str::from_utf8(&data[..head_end]).ok()?;
    let mut words = head.split_whitespace();
    if words.next()? != "<!--" || words.next()? != "dmx" || words.next()? != "encoding" || words.next()? != "binary" {
        return None;
    }
    let version: u32 = words.next()?.parse().ok()?;
    if !(1..=5).contains(&version) {
        return None;
    }
    let body = head_end + 1;
    let candidates: &[Layout] = match version {
        1 | 2 => &[Layout { count_i32: false, index_i32: false, names_indexed: false, values_indexed: false }],
        3 => &[
            Layout { count_i32: false, index_i32: false, names_indexed: false, values_indexed: false },
            Layout { count_i32: true, index_i32: false, names_indexed: false, values_indexed: false },
        ],
        4 => &[
            Layout { count_i32: true, index_i32: false, names_indexed: true, values_indexed: true },
            Layout { count_i32: false, index_i32: false, names_indexed: true, values_indexed: true },
        ],
        _ => &[
            Layout { count_i32: true, index_i32: true, names_indexed: true, values_indexed: true },
            Layout { count_i32: true, index_i32: false, names_indexed: true, values_indexed: true },
        ],
    };
    candidates.iter().find_map(|&l| parse_with(data, body, version, l))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn build_pcf(systems: &[(&str, &str)]) -> Vec<u8> {
        let mut out = b"<!-- dmx encoding binary 2 format pcf 1 -->\n\0".to_vec();
        let table = ["DmElement", "particleSystemDefinitions", "DmeParticleSystemDefinition", "material", "radius"];
        out.extend_from_slice(&(table.len() as u16).to_le_bytes());
        for s in table {
            out.extend_from_slice(s.as_bytes());
            out.push(0);
        }
        out.extend_from_slice(&(systems.len() as i32 + 1).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(b"root\0");
        out.extend_from_slice(&[1; 16]);
        for (name, _) in systems {
            out.extend_from_slice(&2u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.push(0);
            out.extend_from_slice(&[2; 16]);
        }
        out.extend_from_slice(&1i32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.push(ARRAY_BASE + TYPE_ELEMENT);
        out.extend_from_slice(&(systems.len() as i32).to_le_bytes());
        for i in 0..systems.len() {
            out.extend_from_slice(&(i as i32 + 1).to_le_bytes());
        }
        for (_, material) in systems {
            out.extend_from_slice(&2i32.to_le_bytes());
            out.extend_from_slice(&3u16.to_le_bytes());
            out.push(TYPE_STRING);
            out.extend_from_slice(material.as_bytes());
            out.push(0);
            out.extend_from_slice(&4u16.to_le_bytes());
            out.push(3);
            out.extend_from_slice(&5.0f32.to_le_bytes());
        }
        out
    }

    #[test]
    fn reads_systems_and_materials() {
        let data = build_pcf(&[("fire_big", "Particle\\Fire.vmt"), ("smoke", "particle/smoke1")]);
        let pcf = parse_pcf(&data).unwrap();
        assert_eq!(
            pcf.systems,
            vec![
                ("fire_big".to_owned(), "particle/fire".to_owned()),
                ("smoke".to_owned(), "particle/smoke1".to_owned()),
            ]
        );
        assert_eq!(pcf.materials(), ["particle/fire", "particle/smoke1"]);
    }

    #[test]
    fn hostile_files_fail_cleanly() {
        let good = build_pcf(&[("a", "b")]);
        for cut in 0..good.len() {
            assert!(parse_pcf(&good[..cut]).is_none() || cut == good.len());
        }
        for i in 0..good.len() {
            let mut bad = good.clone();
            bad[i] = bad[i].wrapping_add(0x41);
            let _ = parse_pcf(&bad);
        }
        assert!(parse_pcf(b"<!-- dmx encoding keyvalues2 1 format pcf 1 -->\n\0").is_none());
        assert!(parse_pcf(b"").is_none());
    }

    #[test]
    fn real_particle_files() {
        let dir = std::path::PathBuf::from(r"D:\SteamLibrary\steamapps\common\GarrysMod\garrysmod\particles");
        let Ok(read) = std::fs::read_dir(&dir) else { return };
        let mut parsed = 0;
        for entry in read.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "pcf") {
                let data = std::fs::read(&path).unwrap();
                let pcf = parse_pcf(&data).unwrap_or_else(|| panic!("{}", path.display()));
                assert!(!pcf.systems.is_empty(), "{}", path.display());
                parsed += 1;
            }
        }
        assert!(parsed > 0);
    }
}
