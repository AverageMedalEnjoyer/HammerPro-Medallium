use crate::{Material, ShaderKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BlendMode {
    Opaque,
    Alpha,
    Additive,
    AlphaAdditive,
}

impl BlendMode {
    pub fn uses_alpha(self) -> bool {
        matches!(self, Self::Alpha | Self::AlphaAdditive)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlphaUsage {
    pub blend: BlendMode,
    pub alpha_test: Option<f32>,
    pub texture_alpha: bool,
    pub modulation: f32,
}

impl AlphaUsage {
    pub const OPAQUE: Self = Self { blend: BlendMode::Opaque, alpha_test: None, texture_alpha: false, modulation: 1.0 };

    pub fn is_opaque(&self) -> bool {
        self.blend == BlendMode::Opaque && self.alpha_test.is_none()
    }

    pub fn shows_texture_alpha(&self) -> bool {
        self.texture_alpha && (self.blend.uses_alpha() || self.alpha_test.is_some())
    }

    pub fn coverage(&self, alpha: f32) -> f32 {
        let a = if self.texture_alpha { alpha.clamp(0.0, 1.0) } else { 1.0 } * self.modulation;
        if self.alpha_test.is_some_and(|r| a < r) {
            0.0
        } else if self.blend.uses_alpha() {
            a
        } else {
            1.0
        }
    }

    pub fn is_see_through(&self) -> bool {
        (0..=255u8).any(|i| self.coverage(f32::from(i) / 255.0) < 1.0)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Lightmapped,
    VertexLit,
    Skin,
    Unlit,
    UnlitTwoTexture,
    Refract,
    Sprite,
    Other,
}

pub fn quantize_alpha_reference(reference: f32) -> f32 {
    (reference.clamp(0.0, 1.0) * 255.0).floor() / 255.0
}

impl Material {
    pub fn alpha_usage(&self, texture_has_alpha: &dyn Fn(&str) -> bool) -> AlphaUsage {
        let family = match self.shader_kind() {
            ShaderKind::LightmappedGeneric
            | ShaderKind::WorldVertexTransition
            | ShaderKind::Lightmapped4WayBlend
            | ShaderKind::WorldTwoTextureBlend => Family::Lightmapped,
            ShaderKind::VertexLitGeneric if self.flag("$phong") => Family::Skin,
            ShaderKind::VertexLitGeneric => Family::VertexLit,
            ShaderKind::UnlitGeneric => Family::Unlit,
            ShaderKind::UnlitTwoTexture => Family::UnlitTwoTexture,
            ShaderKind::Refract => Family::Refract,
            ShaderKind::Sprite => Family::Sprite,
            _ => Family::Other,
        };
        let base = self.base_texture();
        let has_base = base.is_some();
        let base_flag = base.as_deref().is_some_and(texture_has_alpha);
        let generic = matches!(family, Family::UnlitTwoTexture | Family::Refract | Family::Sprite | Family::Other);

        let self_illum = match family {
            Family::Lightmapped => self.self_illum() && base_flag,
            Family::VertexLit | Family::Skin => {
                let fresnel = self.flag("$selfillumfresnel");
                let mask = self.texture("$selfillummask").is_some();
                self.self_illum() && has_base && (base_flag || fresnel || mask)
            }
            Family::Unlit => false,
            _ => self.self_illum(),
        };
        let mut env_mask = self.flag("$basealphaenvmapmask");
        match family {
            Family::Lightmapped | Family::Skin => env_mask &= has_base && base_flag,
            Family::VertexLit | Family::Unlit => {
                let tangents = self.texture("$bumpmap").is_some() || self.texture("$diffusewarptexture").is_some();
                let normal_mask = self.flag("$normalmapalphaenvmapmask") && tangents;
                env_mask &= has_base && base_flag && !normal_mask;
            }
            _ => {}
        }
        let alpha_masked = self_illum || env_mask;
        let alpha_test = self.alpha_test() && (generic || !alpha_masked);
        let blend_tint = match family {
            Family::VertexLit => self.flag("$blendtintbybasealpha"),
            Family::Skin => self.flag("$blendtintbybasealpha") && !self_illum,
            _ => false,
        };
        let translucent_flag = self.translucent() || family == Family::Refract;
        let vertex_alpha = self.vertex_alpha() || family == Family::Sprite;
        let alpha_modulating = self.alpha() < 1.0;

        let base_texture_translucent =
            |flag: bool| !self.flag("$opaquetexture") && !alpha_masked && (translucent_flag || alpha_test) && flag;
        let texture_translucent = match family {
            Family::UnlitTwoTexture => {
                let tex2 = self.texture("$texture2").is_some_and(|t| texture_has_alpha(&t));
                base_texture_translucent(base_flag) || base_texture_translucent(tex2)
            }
            Family::Refract => self.bump_map().is_some_and(|t| texture_has_alpha(&t)),
            _ if blend_tint => false,
            _ if has_base => base_texture_translucent(base_flag),
            _ => self.texture("$envmapmask").is_some_and(|t| texture_has_alpha(&t)),
        };
        let mut translucent = match family {
            Family::UnlitTwoTexture => alpha_modulating || texture_translucent,
            _ => alpha_modulating || vertex_alpha || (texture_translucent && !alpha_test),
        };
        if !translucent
            && family == Family::VertexLit
            && matches!(self.get_i32("$detailblendmode"), Some(3 | 8 | 9))
            && let Some(detail) = self.texture("$detail")
        {
            translucent = base_texture_translucent(texture_has_alpha(&detail));
        }
        let blend = match (self.additive(), translucent) {
            (false, false) => BlendMode::Opaque,
            (false, true) => BlendMode::Alpha,
            (true, false) => BlendMode::Additive,
            (true, true) => BlendMode::AlphaAdditive,
        };
        let texture_alpha = match family {
            Family::Lightmapped => has_base && !alpha_masked,
            Family::VertexLit | Family::Unlit => has_base && !alpha_masked && !blend_tint,
            Family::Skin => has_base && !self_illum && !blend_tint && !self.flag("$basemapalphaphongmask"),
            Family::Refract => true,
            _ => has_base,
        };
        AlphaUsage {
            blend,
            alpha_test: alpha_test.then(|| quantize_alpha_reference(self.alpha_test_reference())),
            texture_alpha,
            modulation: self.alpha(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn usage(src: &str, alpha: &[&str]) -> AlphaUsage {
        let m = parse(src).unwrap();
        m.alpha_usage(&|t: &str| alpha.contains(&t))
    }

    const REF: f32 = 178.0 / 255.0;

    #[test]
    fn opaque_materials_ignore_texture_alpha() {
        for src in [
            r#"LightmappedGeneric { $basetexture a }"#,
            r#"LightmappedGeneric { $basetexture a $basealphaenvmapmask 1 $envmap env_cubemap }"#,
            r#"LightmappedGeneric { $basetexture a $selfillum 1 }"#,
            r#"WorldVertexTransition { $basetexture a $basetexture2 b $blendmodulatetexture c }"#,
            r#"VertexLitGeneric { $basetexture a $phong 1 $basemapalphaphongmask 1 }"#,
            r#"VertexLitGeneric { $basetexture a $blendtintbybasealpha 1 }"#,
            r#"VertexLitGeneric { $basetexture a $bumpmap n $normalmapalphaenvmapmask 1 }"#,
            r#"UnlitGeneric { $basetexture a }"#,
        ] {
            let u = usage(src, &["a", "b", "n"]);
            assert_eq!(u.blend, BlendMode::Opaque, "{src}");
            assert!(u.is_opaque() && !u.shows_texture_alpha(), "{src}");
        }
    }

    #[test]
    fn translucent_needs_an_alpha_flag() {
        let src = r#"LightmappedGeneric { $basetexture a $translucent 1 }"#;
        let u = usage(src, &["a"]);
        assert_eq!(u.blend, BlendMode::Alpha);
        assert!(u.texture_alpha && u.shows_texture_alpha() && u.alpha_test.is_none());
        assert!(usage(src, &[]).is_opaque());
        assert!(usage(r#"LightmappedGeneric { $basetexture a $translucent 1 $opaquetexture 1 }"#, &["a"]).is_opaque());
        let u = usage(r#"LightmappedGeneric { $translucent 1 $envmapmask m }"#, &["m"]);
        assert_eq!(u.blend, BlendMode::Alpha);
        assert!(!u.texture_alpha);
    }

    #[test]
    fn alpha_test_wins_over_translucent() {
        let u = usage(r#"VertexLitGeneric { $basetexture a $translucent 1 $alphatest 1 }"#, &["a"]);
        assert_eq!(u.blend, BlendMode::Opaque);
        assert_eq!(u.alpha_test, Some(REF));
        assert!(u.texture_alpha && u.shows_texture_alpha());
        let u = usage(r#"LightmappedGeneric { $basetexture a $alphatest 1 $alphatestreference .35 }"#, &[]);
        assert_eq!(u.alpha_test, Some(quantize_alpha_reference(0.35)));
        assert_eq!(u.blend, BlendMode::Opaque);
        let u = usage(r#"LightmappedGeneric { $basetexture a $alphatest 1 $translucent 1 $alpha .5 }"#, &["a"]);
        assert_eq!(u.blend, BlendMode::Alpha);
        assert!(u.alpha_test.is_some());
    }

    #[test]
    fn masks_disable_translucency_and_alpha_test() {
        for src in [
            r#"LightmappedGeneric { $basetexture a $translucent 1 $basealphaenvmapmask 1 }"#,
            r#"LightmappedGeneric { $basetexture a $alphatest 1 $selfillum 1 }"#,
            r#"VertexLitGeneric { $basetexture a $translucent 1 $selfillum 1 }"#,
            r#"VertexLitGeneric { $basetexture a $translucent 1 $basealphaenvmapmask 1 $alphatest 1 }"#,
            r#"VertexLitGeneric { $basetexture a $translucent 1 $blendtintbybasealpha 1 }"#,
        ] {
            let u = usage(src, &["a"]);
            assert!(u.is_opaque(), "{src}: {u:?}");
            assert!(!u.texture_alpha, "{src}");
        }
        let u = usage(r#"LightmappedGeneric { $basetexture a $alphatest 1 $selfillum 1 }"#, &[]);
        assert!(u.alpha_test.is_some() && u.texture_alpha);
        let u = usage(r#"UnlitGeneric { $basetexture a $translucent 1 $selfillum 1 }"#, &["a"]);
        assert_eq!(u.blend, BlendMode::Alpha);
        assert!(u.texture_alpha);
        let u = usage(r#"VertexLitGeneric { $basetexture a $selfillum 1 $selfillummask m $translucent 1 }"#, &[]);
        assert!(!u.texture_alpha);
        let u = usage(r#"LightmappedGeneric { $basetexture a $basealphaenvmapmask 1 $alpha .4 }"#, &["a"]);
        assert_eq!(u.blend, BlendMode::Alpha);
        assert!(!u.texture_alpha && !u.shows_texture_alpha());
        let u = usage(r#"VertexLitGeneric { $basetexture a $phong 1 $basemapalphaphongmask 1 $alpha .5 }"#, &["a"]);
        assert!(!u.texture_alpha);
        let u = usage(r#"VertexLitGeneric { $basetexture a $phong 1 $alpha .5 }"#, &["a"]);
        assert!(u.texture_alpha);
    }

    #[test]
    fn modulation_and_vertex_alpha() {
        let u = usage(r#"LightmappedGeneric { $basetexture a $alpha .5 }"#, &[]);
        assert_eq!(u.blend, BlendMode::Alpha);
        assert!(u.texture_alpha, "the base alpha multiplies $alpha in the pixel shader");
        let u = usage(r#"UnlitGeneric { $basetexture a $vertexalpha 1 }"#, &[]);
        assert_eq!(u.blend, BlendMode::Alpha);
        assert_eq!(usage(r#"Sprite { $basetexture a }"#, &[]).blend, BlendMode::Alpha);
    }

    #[test]
    fn additive() {
        let u = usage(r#"UnlitGeneric { $basetexture a $additive 1 }"#, &["a"]);
        assert_eq!(u.blend, BlendMode::Additive);
        assert!(!u.shows_texture_alpha());
        let u = usage(r#"UnlitGeneric { $basetexture a $additive 1 $translucent 1 }"#, &["a"]);
        assert_eq!(u.blend, BlendMode::AlphaAdditive);
        assert!(u.shows_texture_alpha());
        assert_eq!(
            usage(r#"UnlitGeneric { $basetexture a $additive 1 $alpha .5 }"#, &[]).blend,
            BlendMode::AlphaAdditive
        );
    }

    #[test]
    fn other_shaders() {
        let u = usage(r#"UnlitTwoTexture { $basetexture a $texture2 b $translucent 1 }"#, &["b"]);
        assert_eq!(u.blend, BlendMode::Alpha);
        let u = usage(r#"UnlitTwoTexture { $basetexture a $texture2 b $translucent 1 $alphatest 1 }"#, &["a"]);
        assert_eq!(u.blend, BlendMode::Alpha, "UnlitTwoTexture blends even when alpha tested");
        assert_eq!(usage(r#"Refract { $normalmap n }"#, &["n"]).blend, BlendMode::Alpha);
        assert!(usage(r#"Refract { $normalmap n }"#, &[]).is_opaque());
        let d = r#"VertexLitGeneric { $basetexture a $detail d $detailblendmode 3 $translucent 1 }"#;
        assert_eq!(usage(d, &["d"]).blend, BlendMode::Alpha);
        assert!(usage(d, &[]).is_opaque());
    }

    #[test]
    fn coverage() {
        let glass = usage(r#"LightmappedGeneric { $basetexture a $translucent 1 $alpha .5 }"#, &["a"]);
        assert!((glass.coverage(0.5) - 0.25).abs() < 1e-6);
        assert!(glass.is_see_through());
        let fence = usage(r#"LightmappedGeneric { $basetexture a $alphatest 1 $alphatestreference .5 }"#, &[]);
        assert_eq!((fence.coverage(0.4), fence.coverage(0.6)), (0.0, 1.0));
        assert!(fence.is_see_through());
        let brick = usage(r#"LightmappedGeneric { $basetexture a $basealphaenvmapmask 1 }"#, &["a"]);
        assert_eq!(brick.coverage(0.0), 1.0);
        assert!(!brick.is_see_through());
        let faded = usage(r#"LightmappedGeneric { $basetexture a $basealphaenvmapmask 1 $alpha .3 }"#, &["a"]);
        assert!((faded.coverage(0.0) - 0.3).abs() < 1e-6, "the mask does not count, $alpha does");
        assert!(!usage(r#"UnlitGeneric { $basetexture a $additive 1 }"#, &["a"]).is_see_through());
        assert!(!AlphaUsage::OPAQUE.is_see_through());
    }

    #[test]
    fn alpha_reference() {
        assert_eq!(quantize_alpha_reference(0.7), REF);
        assert_eq!(quantize_alpha_reference(0.5), 127.0 / 255.0);
        assert_eq!(quantize_alpha_reference(2.0), 1.0);
        let u = usage(r#"LightmappedGeneric { $basetexture a $alphatest 1 $alphatestreference 0 }"#, &["a"]);
        assert_eq!(u.alpha_test, Some(REF), "0 keeps the default reference");
    }
}
