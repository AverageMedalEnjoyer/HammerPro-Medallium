use crate::PcfError;

#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Element(Option<usize>),
    Int(i32),
    Float(f32),
    Bool(bool),
    Str(String),
    Color([u8; 4]),
    Vec2([f32; 2]),
    Vec3([f32; 3]),
    Vec4([f32; 4]),
    Elements(Vec<Option<usize>>),
    Ints(Vec<i32>),
    Floats(Vec<f32>),
    Vec3s(Vec<[f32; 3]>),
    Strs(Vec<String>),
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Element {
    pub kind: String,
    pub name: String,
    pub attrs: Vec<(String, Value)>,
}

impl Element {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.attrs.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v)
    }

    pub fn float(&self, name: &str) -> Option<f32> {
        match self.get(name)? {
            Value::Float(v) => Some(*v),
            Value::Int(v) => Some(*v as f32),
            Value::Bool(b) => Some(f32::from(u8::from(*b))),
            _ => None,
        }
        .filter(|v| v.is_finite())
    }

    pub fn int(&self, name: &str) -> Option<i32> {
        match self.get(name)? {
            Value::Int(v) => Some(*v),
            Value::Float(v) if v.is_finite() => Some(*v as i32),
            Value::Bool(b) => Some(i32::from(*b)),
            _ => None,
        }
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        match self.get(name)? {
            Value::Bool(b) => Some(*b),
            Value::Int(v) => Some(*v != 0),
            Value::Float(v) => Some(*v != 0.0),
            _ => None,
        }
    }

    pub fn vec3(&self, name: &str) -> Option<[f32; 3]> {
        match self.get(name)? {
            Value::Vec3(v) if v.iter().all(|c| c.is_finite()) => Some(*v),
            _ => None,
        }
    }

    pub fn color(&self, name: &str) -> Option<[u8; 4]> {
        match self.get(name)? {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn elements(&self, name: &str) -> Vec<usize> {
        match self.get(name) {
            Some(Value::Elements(v)) => v.iter().flatten().copied().collect(),
            Some(Value::Element(Some(i))) => vec![*i],
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dmx {
    pub version: u32,
    pub elements: Vec<Element>,
}

#[derive(Clone, Copy, Debug)]
struct Layout {
    count_i32: bool,
    index_i32: bool,
    names_indexed: bool,
    values_indexed: bool,
    time_is_int: bool,
}

struct Cur<'a> {
    d: &'a [u8],
    p: usize,
}

impl<'a> Cur<'a> {
    fn left(&self) -> usize {
        self.d.len().saturating_sub(self.p)
    }
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
    fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn count(&mut self, item_min: usize) -> Option<usize> {
        let n = usize::try_from(self.i32()?).ok()?;
        (n.saturating_mul(item_min.max(1)) <= self.left()).then_some(n)
    }
    fn cstr(&mut self) -> Option<&'a [u8]> {
        let rest = self.d.get(self.p..)?;
        let n = rest.iter().position(|&b| b == 0)?;
        self.p += n + 1;
        Some(&rest[..n])
    }
}

const TYPE_ELEMENT: u8 = 1;
const TYPE_INT: u8 = 2;
const TYPE_FLOAT: u8 = 3;
const TYPE_BOOL: u8 = 4;
const TYPE_STRING: u8 = 5;
const TYPE_VOID: u8 = 6;
const TYPE_TIME: u8 = 7;
const TYPE_COLOR: u8 = 8;
const TYPE_VEC2: u8 = 9;
const TYPE_VEC3: u8 = 10;
const TYPE_VEC4: u8 = 11;
const TYPE_QANGLE: u8 = 12;
const TYPE_QUAT: u8 = 13;
const TYPE_MATRIX: u8 = 14;
const ARRAY_BASE: u8 = 14;

fn text(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

fn fixed_size(t: u8, layout: Layout) -> Option<usize> {
    Some(match t {
        TYPE_ELEMENT | TYPE_INT | TYPE_FLOAT | TYPE_COLOR => 4,
        TYPE_BOOL => 1,
        TYPE_TIME => {
            if layout.time_is_int {
                4
            } else {
                16
            }
        }
        TYPE_VEC2 => 8,
        TYPE_VEC3 | TYPE_QANGLE => 12,
        TYPE_VEC4 | TYPE_QUAT => 16,
        TYPE_MATRIX => 64,
        _ => return None,
    })
}

fn element_ref(c: &mut Cur<'_>, count: usize) -> Option<Option<usize>> {
    let i = c.i32()?;
    if i == -2 {
        c.cstr()?;
        return Some(None);
    }
    Some(usize::try_from(i).ok().filter(|&i| i < count))
}

fn read_value(c: &mut Cur<'_>, t: u8, layout: Layout, table: &[&[u8]], count: usize) -> Option<Value> {
    let index = |c: &mut Cur<'_>| -> Option<usize> {
        if layout.index_i32 { usize::try_from(c.i32()?).ok() } else { c.u16().map(usize::from) }
    };
    Some(match t {
        TYPE_ELEMENT => Value::Element(element_ref(c, count)?),
        TYPE_INT => Value::Int(c.i32()?),
        TYPE_FLOAT => Value::Float(c.f32()?),
        TYPE_BOOL => Value::Bool(c.u8()? != 0),
        TYPE_STRING => {
            let s = if layout.values_indexed { *table.get(index(c)?)? } else { c.cstr()? };
            Value::Str(text(s))
        }
        TYPE_VOID => {
            let n = c.count(1)?;
            c.take(n)?;
            Value::Other
        }
        TYPE_COLOR => {
            let b = c.take(4)?;
            Value::Color([b[0], b[1], b[2], b[3]])
        }
        TYPE_VEC2 => Value::Vec2([c.f32()?, c.f32()?]),
        TYPE_VEC3 | TYPE_QANGLE => Value::Vec3([c.f32()?, c.f32()?, c.f32()?]),
        TYPE_VEC4 | TYPE_QUAT => Value::Vec4([c.f32()?, c.f32()?, c.f32()?, c.f32()?]),
        t if t > ARRAY_BASE => {
            let base = t - ARRAY_BASE;
            match base {
                TYPE_ELEMENT => {
                    let n = c.count(4)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push(element_ref(c, count)?);
                    }
                    Value::Elements(v)
                }
                TYPE_INT => {
                    let n = c.count(4)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push(c.i32()?);
                    }
                    Value::Ints(v)
                }
                TYPE_FLOAT => {
                    let n = c.count(4)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push(c.f32()?);
                    }
                    Value::Floats(v)
                }
                TYPE_VEC3 => {
                    let n = c.count(12)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push([c.f32()?, c.f32()?, c.f32()?]);
                    }
                    Value::Vec3s(v)
                }
                TYPE_STRING => {
                    let n = c.count(1)?;
                    let mut v = Vec::with_capacity(n);
                    for _ in 0..n {
                        v.push(text(c.cstr()?));
                    }
                    Value::Strs(v)
                }
                TYPE_VOID => {
                    let n = c.count(4)?;
                    for _ in 0..n {
                        let k = c.count(1)?;
                        c.take(k)?;
                    }
                    Value::Other
                }
                b => {
                    let size = fixed_size(b, layout)?;
                    let n = c.count(size)?;
                    c.take(n.checked_mul(size)?)?;
                    Value::Other
                }
            }
        }
        t => {
            c.take(fixed_size(t, layout)?)?;
            Value::Other
        }
    })
}

fn parse_with(data: &[u8], body: usize, version: u32, layout: Layout) -> Option<Dmx> {
    let mut c = Cur { d: data, p: body };
    let mut table: Vec<&[u8]> = Vec::new();
    if version >= 2 {
        let n = if layout.count_i32 { c.count(1)? } else { usize::from(c.u16()?) };
        if n > c.left() {
            return None;
        }
        table.reserve(n);
        for _ in 0..n {
            table.push(c.cstr()?);
        }
    }
    let index = |c: &mut Cur<'_>| -> Option<usize> {
        if layout.index_i32 { usize::try_from(c.i32()?).ok() } else { c.u16().map(usize::from) }
    };
    let count = c.count(17)?;
    let mut elements: Vec<Element> = Vec::with_capacity(count);
    for _ in 0..count {
        let kind = if version >= 2 { *table.get(index(&mut c)?)? } else { c.cstr()? };
        let name = if layout.names_indexed { *table.get(index(&mut c)?)? } else { c.cstr()? };
        c.take(16)?;
        elements.push(Element { kind: text(kind), name: text(name), attrs: Vec::new() });
    }
    for e in elements.iter_mut() {
        let n = c.count(2)?;
        e.attrs.reserve(n);
        for _ in 0..n {
            let name = if version >= 2 { *table.get(index(&mut c)?)? } else { c.cstr()? };
            let t = c.u8()?;
            let v = read_value(&mut c, t, layout, &table, count)?;
            e.attrs.push((text(name), v));
        }
    }
    (c.p == data.len()).then_some(Dmx { version, elements })
}

pub fn parse(data: &[u8]) -> Result<Dmx, PcfError> {
    let head_end = data.iter().take(256).position(|&b| b == 0).ok_or(PcfError::NotDmx)?;
    let head = std::str::from_utf8(&data[..head_end]).map_err(|_| PcfError::NotDmx)?;
    let mut words = head.split_whitespace();
    let ok = words.next() == Some("<!--")
        && words.next() == Some("dmx")
        && words.next() == Some("encoding")
        && words.next() == Some("binary");
    if !ok {
        return Err(PcfError::NotDmx);
    }
    let version: u32 = words.next().and_then(|w| w.parse().ok()).ok_or(PcfError::NotDmx)?;
    if !(1..=5).contains(&version) {
        return Err(PcfError::Unsupported(version));
    }
    let body = head_end + 1;
    let l = |count_i32, index_i32, names_indexed, values_indexed, time_is_int| Layout {
        count_i32,
        index_i32,
        names_indexed,
        values_indexed,
        time_is_int,
    };
    let candidates: Vec<Layout> = match version {
        1 | 2 => vec![l(false, false, false, false, false)],
        3 => vec![
            l(false, false, false, false, true),
            l(true, false, false, false, true),
            l(false, false, false, false, false),
            l(true, false, false, false, false),
        ],
        4 => vec![
            l(true, false, true, true, true),
            l(false, false, true, true, true),
            l(true, false, true, true, false),
            l(false, false, true, true, false),
        ],
        _ => vec![
            l(true, true, true, true, true),
            l(true, false, true, true, true),
            l(true, true, true, true, false),
            l(true, false, true, true, false),
        ],
    };
    candidates.into_iter().find_map(|layout| parse_with(data, body, version, layout)).ok_or(PcfError::Corrupt)
}
