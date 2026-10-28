use hp_kv::Block;

use crate::Material;
use crate::value::normalize_texture_path;

pub const DEFAULT_IMAGE_SHADER: &str = "LightmappedGeneric";

#[inline]
fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push('\t');
    }
}

#[inline]
fn push_quoted(out: &mut String, s: &str) {
    out.push('"');
    if s.contains('"') {
        out.push_str(&s.replace('"', "'"));
    } else {
        out.push_str(s);
    }
    out.push('"');
}

fn write_pair(out: &mut String, indent: usize, key: &str, value: &str) {
    push_indent(out, indent);
    push_quoted(out, key);
    out.push(' ');
    push_quoted(out, value);
    out.push('\n');
}

fn write_block(out: &mut String, indent: usize, name: &str, block: &Block) {
    push_indent(out, indent);
    push_quoted(out, name);
    out.push('\n');
    push_indent(out, indent);
    out.push_str("{\n");
    for (k, v) in &block.props {
        write_pair(out, indent + 1, k, v);
    }
    for child in &block.children {
        write_block(out, indent + 1, &child.name, child);
    }
    push_indent(out, indent);
    out.push_str("}\n");
}

pub fn write(mat: &Material) -> String {
    let mut out = String::with_capacity(64 + mat.params.len() * 40);
    push_quoted(&mut out, &mat.shader);
    out.push_str("\n{\n");
    for (k, v) in &mat.params {
        write_pair(&mut out, 1, k, v);
    }
    let mut first_block = true;
    let mut separate = |out: &mut String| {
        if first_block && !mat.params.is_empty() {
            out.push('\n');
        }
        first_block = false;
    };
    if let Some(proxies) = &mat.proxies {
        separate(&mut out);
        let name = if proxies.name.eq_ignore_ascii_case("proxies") { proxies.name.as_str() } else { "Proxies" };
        write_block(&mut out, 1, name, proxies);
    }
    for block in &mat.other_blocks {
        separate(&mut out);
        write_block(&mut out, 1, &block.name, block);
    }
    out.push_str("}\n");
    out
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageMaterialOptions {
    pub shader: String,
    pub surfaceprop: Option<String>,
    pub translucent: bool,
    pub alphatest: bool,
    pub nocull: bool,
}

impl Default for ImageMaterialOptions {
    fn default() -> Self {
        Self {
            shader: DEFAULT_IMAGE_SHADER.to_owned(),
            surfaceprop: None,
            translucent: false,
            alphatest: false,
            nocull: false,
        }
    }
}

pub fn for_image(texture_path: &str, opts: &ImageMaterialOptions) -> Material {
    let shader = opts.shader.trim();
    let shader = if shader.is_empty() { DEFAULT_IMAGE_SHADER } else { shader };
    let mut mat = Material::new(shader);
    mat.params.push(("$basetexture".to_owned(), normalize_texture_path(texture_path)));
    if let Some(sp) = opts.surfaceprop.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        mat.params.push(("$surfaceprop".to_owned(), sp.to_owned()));
    }
    if opts.alphatest {
        mat.params.push(("$alphatest".to_owned(), "1".to_owned()));
    } else if opts.translucent {
        mat.params.push(("$translucent".to_owned(), "1".to_owned()));
    }
    if opts.nocull {
        mat.params.push(("$nocull".to_owned(), "1".to_owned()));
    }
    mat
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    #[test]
    fn writes_clean_text() {
        let m = parse(
            "LightmappedGeneric { $basetexture concrete/floor $color \"[1 1 1]\" Proxies { Sine { resultvar $alpha } } LightmappedGeneric_DX9 { $x 1 } }",
        )
        .unwrap();
        let text = write(&m);
        let expected = "\"LightmappedGeneric\"\n{\n\t\"$basetexture\" \"concrete/floor\"\n\t\"$color\" \"[1 1 1]\"\n\n\t\"Proxies\"\n\t{\n\t\t\"Sine\"\n\t\t{\n\t\t\t\"resultvar\" \"$alpha\"\n\t\t}\n\t}\n\t\"LightmappedGeneric_DX9\"\n\t{\n\t\t\"$x\" \"1\"\n\t}\n}\n";
        assert_eq!(text, expected);
        assert_eq!(parse(&text).unwrap(), m);
    }

    #[test]
    fn roundtrip_edge_cases() {
        let mut m = Material::new("");
        m.params.push((String::new(), String::new()));
        m.params.push(("$a b".to_owned(), "{1 2 3}".to_owned()));
        m.params.push(("$c".to_owned(), "x // not a comment".to_owned()));
        m.params.push(("$d".to_owned(), "multi\nline".to_owned()));
        m.params.push(("$dup".to_owned(), "1".to_owned()));
        m.params.push(("$DUP".to_owned(), "2".to_owned()));
        m.other_blocks.push(Block::new("proxies"));
        m.other_blocks.push(Block::new(">=dx90"));
        let mut p = Block::new("");
        p.push_child(Block::new("x y"));
        m.proxies = Some(p);
        let text = write(&m);
        let back = parse(&text).unwrap();
        assert_eq!(back.params, m.params);
        assert_eq!(back.other_blocks, m.other_blocks);
        assert_eq!(back.proxies.as_ref().unwrap().name, "Proxies");
        assert!(back.proxies.as_ref().unwrap().child("x y").is_some());
        assert_eq!(write(&back), write(&parse(&write(&back)).unwrap()));
    }

    #[test]
    fn quotes_are_sanitized() {
        let mut m = Material::new("UnlitGeneric");
        m.set("$basetexture", "a\"b");
        let text = write(&m);
        assert_eq!(parse(&text).unwrap().get("$basetexture"), Some("a'b"));
    }

    #[test]
    fn empty_material() {
        let text = write(&Material::new("UnlitGeneric"));
        assert_eq!(text, "\"UnlitGeneric\"\n{\n}\n");
        let m = Material { shader: "X".into(), proxies: Some(Block::new("Proxies")), ..Material::default() };
        assert_eq!(write(&m), "\"X\"\n{\n\t\"Proxies\"\n\t{\n\t}\n}\n");
    }

    #[test]
    fn image_materials() {
        let m = for_image("Materials\\Custom\\Brick01.PNG", &ImageMaterialOptions::default());
        assert_eq!(m.shader, "LightmappedGeneric");
        assert_eq!(m.base_texture().as_deref(), Some("custom/brick01"));
        assert_eq!(m.params.len(), 1);

        let opts = ImageMaterialOptions {
            shader: "  ".into(),
            surfaceprop: Some(" metal ".into()),
            translucent: true,
            alphatest: false,
            nocull: true,
        };
        let m = for_image("custom/fence.tga", &opts);
        assert_eq!(m.shader, "LightmappedGeneric");
        assert_eq!(m.surface_prop(), Some("metal"));
        assert!(m.translucent() && m.no_cull() && !m.alpha_test());

        let opts = ImageMaterialOptions {
            shader: "VertexLitGeneric".into(),
            surfaceprop: Some(String::new()),
            translucent: true,
            alphatest: true,
            nocull: false,
        };
        let m = for_image("custom/leaves.png", &opts);
        assert_eq!(m.shader, "VertexLitGeneric");
        assert!(m.alpha_test() && !m.translucent());
        assert_eq!(m.surface_prop(), None);
        assert_eq!(parse(&write(&m)).unwrap(), m);
    }
}
