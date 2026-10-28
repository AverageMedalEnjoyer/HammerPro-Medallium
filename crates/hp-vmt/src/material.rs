use hp_kv::Block;

use crate::VmtError;
use crate::value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Material {
    pub shader: String,
    pub params: Vec<(String, String)>,
    pub proxies: Option<Block>,
    pub other_blocks: Vec<Block>,
}

pub fn parse(src: &str) -> Result<Material, VmtError> {
    let root = hp_kv::parse(src)?;
    let block = root.children.into_iter().next().ok_or(VmtError::NoShader)?;
    Ok(Material::from_block(block))
}

pub fn parse_bytes(bytes: &[u8]) -> Result<Material, VmtError> {
    if let Some(text) = decode_utf16(bytes) {
        return parse(&text);
    }
    match std::str::from_utf8(bytes) {
        Ok(s) => parse(s),
        Err(_) => parse(&String::from_utf8_lossy(bytes)),
    }
}

pub(crate) fn decode_utf16(bytes: &[u8]) -> Option<String> {
    let (little_endian, rest) = match bytes {
        [0xFF, 0xFE, rest @ ..] => (true, rest),
        [0xFE, 0xFF, rest @ ..] => (false, rest),
        _ => return None,
    };
    let units = rest.chunks_exact(2).map(|pair| {
        let (a, b) = match *pair {
            [a, b] => (a, b),
            _ => (0, 0),
        };
        if little_endian { u16::from_le_bytes([a, b]) } else { u16::from_be_bytes([a, b]) }
    });
    Some(char::decode_utf16(units).map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER)).collect())
}

pub(crate) fn set_unique(props: &mut Vec<(String, String)>, key: &str, value: &str) {
    match props.iter().position(|(k, _)| k.eq_ignore_ascii_case(key)) {
        Some(first) => {
            if let Some(slot) = props.get_mut(first) {
                value.clone_into(&mut slot.1);
            }
            let mut index = 0usize;
            props.retain(|(k, _)| {
                let keep = index <= first || !k.eq_ignore_ascii_case(key);
                index += 1;
                keep
            });
        }
        None => props.push((key.to_owned(), value.to_owned())),
    }
}

pub(crate) fn remove_all(props: &mut Vec<(String, String)>, key: &str) -> Option<String> {
    let mut first = None;
    props.retain_mut(|(k, v)| {
        if k.eq_ignore_ascii_case(key) {
            if first.is_none() {
                first = Some(std::mem::take(v));
            }
            false
        } else {
            true
        }
    });
    first
}

impl Material {
    pub fn new(shader: impl Into<String>) -> Self {
        Self { shader: shader.into(), ..Self::default() }
    }

    pub fn from_block(block: Block) -> Self {
        let Block { name, props, children } = block;
        let mut proxies = None;
        let mut other_blocks = Vec::new();
        for child in children {
            if proxies.is_none() && child.name.eq_ignore_ascii_case("proxies") {
                proxies = Some(child);
            } else {
                other_blocks.push(child);
            }
        }
        Self { shader: name, params: props, proxies, other_blocks }
    }

    pub fn to_block(&self) -> Block {
        let mut children = Vec::with_capacity(self.other_blocks.len() + 1);
        if let Some(p) = &self.proxies {
            children.push(p.clone());
        }
        children.extend(self.other_blocks.iter().cloned());
        Block { name: self.shader.clone(), props: self.params.clone(), children }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.params.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }

    pub fn has(&self, key: &str) -> bool {
        self.params.iter().any(|(k, _)| k.eq_ignore_ascii_case(key))
    }

    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.get(key).and_then(value::parse_f32)
    }

    pub fn get_i32(&self, key: &str) -> Option<i32> {
        self.get(key).and_then(value::parse_i32)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get(key).and_then(value::parse_bool)
    }

    pub fn flag(&self, key: &str) -> bool {
        self.get_bool(key).unwrap_or(false)
    }

    pub fn get_vec3(&self, key: &str) -> Option<[f32; 3]> {
        self.get(key).and_then(value::parse_vec3)
    }

    pub fn set(&mut self, key: impl AsRef<str>, value: impl AsRef<str>) {
        set_unique(&mut self.params, key.as_ref(), value.as_ref());
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        remove_all(&mut self.params, key)
    }

    pub fn block(&self, name: &str) -> Option<&Block> {
        self.other_blocks.iter().find(|b| b.name.eq_ignore_ascii_case(name))
    }

    pub fn is_patch(&self) -> bool {
        self.shader.trim().eq_ignore_ascii_case("patch")
    }

    pub fn set_patch_aware(&mut self, key: &str, value: &str) {
        if !self.is_patch() || key.eq_ignore_ascii_case("include") {
            self.set(key, value);
            return;
        }
        let replace = self.other_blocks.iter().position(|b| b.name.eq_ignore_ascii_case("replace"));
        if let Some(i) = replace
            && let Some(block) = self.other_blocks.get_mut(i)
            && block.get(key).is_some()
        {
            set_unique(&mut block.props, key, value);
            return;
        }
        match self.other_blocks.iter_mut().find(|b| b.name.eq_ignore_ascii_case("insert")) {
            Some(block) => set_unique(&mut block.props, key, value),
            None => {
                let mut block = Block::new("insert");
                block.props.push((key.to_owned(), value.to_owned()));
                self.other_blocks.push(block);
            }
        }
    }

    pub fn remove_patch_aware(&mut self, key: &str) -> bool {
        if !self.is_patch() || key.eq_ignore_ascii_case("include") {
            return self.remove(key).is_some();
        }
        let mut removed = false;
        for block in &mut self.other_blocks {
            if block.name.eq_ignore_ascii_case("insert") || block.name.eq_ignore_ascii_case("replace") {
                removed |= remove_all(&mut block.props, key).is_some();
            }
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
// leading comment
"LightmappedGeneric"
{
	"$basetexture" "Concrete/Floor01"
	$surfaceprop concrete
	"$color" "{255 128 0}"
	"$alpha" ".5"
	"$translucent" 1
	"$envmap" "env_cubemap" [!$X360]
	"LightmappedGeneric_DX9"
	{
		"$envmaptint" "[.5 .5 .5]"
	}
	"Proxies"
	{
		"AnimatedTexture"
		{
			"animatedtexturevar" "$basetexture"
		}
	}
}
"#;

    #[test]
    fn parse_basic() {
        let m = parse(SAMPLE).unwrap();
        assert_eq!(m.shader, "LightmappedGeneric");
        assert_eq!(m.params.len(), 6);
        assert_eq!(m.get("$BASETEXTURE"), Some("Concrete/Floor01"));
        assert_eq!(m.get("$surfaceprop"), Some("concrete"));
        assert_eq!(m.get_f32("$alpha"), Some(0.5));
        assert_eq!(m.get_bool("$translucent"), Some(true));
        assert!(m.flag("$translucent"));
        assert!(!m.flag("$nocull"));
        assert_eq!(m.get("$envmap"), Some("env_cubemap"));
        let c = m.get_vec3("$color").unwrap();
        assert!((c[0] - 1.0).abs() < 1e-6 && (c[1] - 128.0 / 255.0).abs() < 1e-6 && c[2] == 0.0);
        assert_eq!(m.other_blocks.len(), 1);
        assert_eq!(m.other_blocks[0].name, "LightmappedGeneric_DX9");
        assert!(m.block("lightmappedgeneric_dx9").is_some());
        let proxies = m.proxies.as_ref().unwrap();
        assert!(proxies.child("animatedtexture").is_some());
        assert!(!m.is_patch());
    }

    #[test]
    fn parse_errors_and_leniency() {
        assert!(matches!(parse(""), Err(VmtError::NoShader)));
        assert!(matches!(parse("// only a comment"), Err(VmtError::NoShader)));
        assert!(matches!(parse("\"$basetexture\" \"x\""), Err(VmtError::NoShader)));
        let m = parse("\u{feff}} UnlitGeneric { $basetexture a/b ").unwrap();
        assert_eq!(m.shader, "UnlitGeneric");
        assert_eq!(m.get("$basetexture"), Some("a/b"));
        let m = parse("A { k 1 } B { k 2 }").unwrap();
        assert_eq!(m.shader, "A");
        assert_eq!(m.get("k"), Some("1"));
    }

    #[test]
    fn parse_bytes_encodings() {
        let utf8 = parse_bytes(b"UnlitGeneric { $basetexture \"a\" }").unwrap();
        assert_eq!(utf8.get("$basetexture"), Some("a"));
        let latin1 = parse_bytes(b"UnlitGeneric { // caf\xe9\n $basetexture \"a\" }").unwrap();
        assert_eq!(latin1.get("$basetexture"), Some("a"));
        let text = "\"VertexLitGeneric\" { \"$basetexture\" \"models/x\" }";
        let mut le = vec![0xFF, 0xFE];
        let mut be = vec![0xFE, 0xFF];
        for u in text.encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
            be.extend_from_slice(&u.to_be_bytes());
        }
        le.push(0x41);
        for bytes in [&le, &be] {
            let m = parse_bytes(bytes).unwrap();
            assert_eq!(m.shader, "VertexLitGeneric");
            assert_eq!(m.get("$basetexture"), Some("models/x"));
        }
    }

    #[test]
    fn set_and_remove() {
        let mut m = parse("X { $A 1 $b 2 $a 3 }").unwrap();
        m.set("$a", "9");
        assert_eq!(m.params, vec![("$A".to_owned(), "9".to_owned()), ("$b".to_owned(), "2".to_owned())]);
        m.set("$new", "x");
        assert_eq!(m.params.last().unwrap().0, "$new");
        assert_eq!(m.remove("$B"), Some("2".to_owned()));
        assert_eq!(m.remove("$missing"), None);
        assert!(!m.has("$b"));
        assert!(m.has("$NEW"));
    }

    #[test]
    fn block_roundtrip() {
        let m = parse(SAMPLE).unwrap();
        let back = Material::from_block(m.to_block());
        assert_eq!(back, m);
        let empty = Material::new("UnlitGeneric");
        assert_eq!(empty.shader, "UnlitGeneric");
        assert!(empty.params.is_empty() && empty.proxies.is_none() && empty.other_blocks.is_empty());
    }

    #[test]
    fn duplicate_proxies_go_to_other_blocks() {
        let m = parse("X { Proxies { A { } } proxies { B { } } }").unwrap();
        assert!(m.proxies.as_ref().unwrap().child("a").is_some());
        assert_eq!(m.other_blocks.len(), 1);
    }

    #[test]
    fn patch_aware_edits() {
        let src = "patch { include \"materials/a/b.vmt\" replace { $envmap x } insert { $detail y } }";
        let mut m = parse(src).unwrap();
        assert!(m.is_patch());
        m.set_patch_aware("$envmap", "z");
        assert_eq!(m.block("replace").unwrap().get("$envmap"), Some("z"));
        m.set_patch_aware("$surfaceprop", "metal");
        assert_eq!(m.block("insert").unwrap().get("$surfaceprop"), Some("metal"));
        m.set_patch_aware("include", "materials/c.vmt");
        assert_eq!(m.get("include"), Some("materials/c.vmt"));
        assert!(m.remove_patch_aware("$detail"));
        assert!(m.remove_patch_aware("$envmap"));
        assert!(!m.remove_patch_aware("$nothing"));
        assert!(m.block("insert").unwrap().get("$detail").is_none());

        let mut bare = parse("patch { include \"materials/a.vmt\" }").unwrap();
        bare.set_patch_aware("$basetexture", "q");
        assert_eq!(bare.block("insert").unwrap().get("$basetexture"), Some("q"));

        let mut regular = parse("X { $a 1 }").unwrap();
        regular.set_patch_aware("$a", "2");
        assert_eq!(regular.get("$a"), Some("2"));
        assert!(regular.remove_patch_aware("$a"));
    }
}
