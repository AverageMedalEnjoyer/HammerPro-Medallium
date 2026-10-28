use std::path::{Path, PathBuf};

use ahash::AHashSet;
use hp_app::state::ImportOptions;
use hp_app::tf;
use rayon::prelude::*;

use crate::AssetManager;
use crate::jobs::JobResult;
use crate::registry::spawn_task;
use crate::util::{normalize_prefix, sanitize_stem};

fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension(format!(
        "{}.tmp",
        path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
    ));
    let shown = path.display().to_string();
    std::fs::write(&tmp, data).map_err(|e| tf!("assets.import_write_failed", shown, e))?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        tf!("assets.import_write_failed", shown, e)
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AlphaKind {
    Opaque,
    Binary,
    Blended,
}

impl AlphaKind {
    pub(crate) fn of_pixels(rgba: &[u8]) -> Self {
        let mut kind = Self::Opaque;
        for px in rgba.chunks_exact(4) {
            match px[3] {
                255 => {}
                0 => kind = Self::Binary,
                _ => return Self::Blended,
            }
        }
        kind
    }
}

fn plan(file: &Path, prefix: &str) -> Result<(String, String), String> {
    let shown = file.display().to_string();
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(sanitize_stem)
        .ok_or_else(|| tf!("assets.import_bad_name", shown))?;
    let name = if prefix.is_empty() { stem.clone() } else { format!("{prefix}/{stem}") };
    Ok((name, stem))
}

fn import_one(file: &Path, name: &str, stem: &str, opts: &ImportOptions) -> Result<String, String> {
    let shown = file.display().to_string();
    let vtf_path = opts.dest_dir.join(format!("{stem}.vtf"));
    let vmt_path = opts.dest_dir.join(format!("{stem}.vmt"));
    if !opts.overwrite && (vtf_path.exists() || vmt_path.exists()) {
        return Err(tf!("assets.import_exists", name));
    }
    let bytes = std::fs::read(file).map_err(|e| tf!("assets.import_read_failed", shown, e))?;
    let ext = file.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let img = hp_vtf::load_image_file(&bytes, &ext).map_err(|e| tf!("assets.import_decode_failed", shown, e))?;
    let eopts = hp_vtf::EncodeOptions {
        format: hp_vtf::EncodeFormat::Auto,
        mipmaps: opts.generate_mipmaps,
        quality: hp_vtf::EncodeQuality::Normal,
        ..hp_vtf::EncodeOptions::default()
    };
    let vtf = hp_vtf::encode(&img, &eopts).map_err(|e| tf!("assets.import_encode_failed", shown, e))?;
    let alpha = AlphaKind::of_pixels(&img.pixels);
    let mopts = hp_vmt::ImageMaterialOptions {
        surfaceprop: opts.surface_prop.clone().filter(|s| !s.trim().is_empty()),
        alphatest: alpha == AlphaKind::Binary,
        translucent: alpha == AlphaKind::Blended,
        ..hp_vmt::ImageMaterialOptions::default()
    };
    let vmt = hp_vmt::write(&hp_vmt::for_image(name, &mopts));
    write_atomic(&vtf_path, &vtf)?;
    write_atomic(&vmt_path, vmt.as_bytes())?;
    Ok(name.to_owned())
}

pub fn import_files(files: &[PathBuf], opts: &ImportOptions) -> Vec<Result<String, String>> {
    let prefix = normalize_prefix(&opts.prefix);
    if let Err(e) = std::fs::create_dir_all(&opts.dest_dir) {
        let dest = opts.dest_dir.display().to_string();
        return files.iter().map(|_| Err(tf!("assets.import_folder_failed", dest, e))).collect();
    }
    let mut seen = AHashSet::new();
    let plans: Vec<Result<(String, String), String>> = files
        .iter()
        .map(|f| {
            let (name, stem) = plan(f, &prefix)?;
            if seen.insert(stem.clone()) { Ok((name, stem)) } else { Err(tf!("assets.import_exists", name)) }
        })
        .collect();
    files
        .par_iter()
        .zip(plans.into_par_iter())
        .map(|(f, p)| {
            let (name, stem) = p?;
            import_one(f, &name, &stem, opts)
        })
        .collect()
}

impl AssetManager {
    pub(crate) fn import_images_impl(
        &mut self,
        files: &[PathBuf],
        opts: &ImportOptions,
    ) -> Vec<Result<String, String>> {
        let results = import_files(files, opts);
        let names: Vec<String> = results.iter().filter_map(|r| r.as_ref().ok().cloned()).collect();
        if !names.is_empty() {
            self.generation += 1;
            if let Some(fs) = self.fs.clone() {
                let names2 = names.clone();
                spawn_task(
                    "hp-assets-refresh",
                    &self.shared,
                    move || {
                        fs.refresh();
                        JobResult::Refreshed { names }
                    },
                    move || JobResult::Refreshed { names: names2 },
                );
            }
        }
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/hp-assets/scratch").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    fn png(path: &Path, w: u32, h: u32, alpha: u8) {
        let px: Vec<u8> = [120u8, 60, 30, alpha].repeat((w * h) as usize);
        let _ = image::save_buffer(path, &px, w, h, image::ColorType::Rgba8);
    }

    fn cutout_png(path: &Path, w: u32, h: u32) {
        let px: Vec<u8> = (0..w * h).flat_map(|i| [10u8, 20, 30, if i % 2 == 0 { 255 } else { 0 }]).collect();
        let _ = image::save_buffer(path, &px, w, h, image::ColorType::Rgba8);
    }

    #[test]
    fn alpha_classification() {
        assert_eq!(AlphaKind::of_pixels(&[1, 2, 3, 255, 4, 5, 6, 255]), AlphaKind::Opaque);
        assert_eq!(AlphaKind::of_pixels(&[1, 2, 3, 255, 4, 5, 6, 0]), AlphaKind::Binary);
        assert_eq!(AlphaKind::of_pixels(&[1, 2, 3, 0, 4, 5, 6, 128]), AlphaKind::Blended);
        assert_eq!(AlphaKind::of_pixels(&[]), AlphaKind::Opaque);
    }

    #[test]
    fn imports_images() {
        hp_app::register_translations();
        crate::register_translations();
        let dir = scratch("import");
        let src = dir.join("src");
        let _ = std::fs::create_dir_all(&src);
        png(&src.join("Brick Wall.png"), 64, 32, 255);
        png(&src.join("glass.png"), 32, 32, 100);
        cutout_png(&src.join("fence.png"), 32, 32);
        std::fs::write(src.join("broken.png"), b"not an image").unwrap_or_default();
        let dest = dir.join("materials").join("custom").join("test");
        let opts = ImportOptions {
            dest_dir: dest.clone(),
            prefix: "Custom/Test/".into(),
            surface_prop: Some("brick".into()),
            generate_mipmaps: true,
            overwrite: false,
        };
        let files = vec![
            src.join("Brick Wall.png"),
            src.join("fence.png"),
            src.join("broken.png"),
            src.join("___.png"),
            src.join("glass.png"),
        ];
        let r = import_files(&files, &opts);
        assert_eq!(r[0].as_deref(), Ok("custom/test/brick_wall"));
        assert_eq!(r[1].as_deref(), Ok("custom/test/fence"));
        assert!(r[2].is_err());
        assert!(r[3].is_err());
        let vtf = std::fs::read(dest.join("brick_wall.vtf")).unwrap_or_default();
        let info = hp_vtf::read_info(&vtf).unwrap();
        assert_eq!((info.width, info.height), (64, 32));
        assert!(info.mip_count > 1);
        let vmt = std::fs::read_to_string(dest.join("brick_wall.vmt")).unwrap_or_default();
        let m = hp_vmt::parse(&vmt).unwrap();
        assert_eq!(m.base_texture().as_deref(), Some("custom/test/brick_wall"));
        assert_eq!(m.surface_prop(), Some("brick"));
        assert!(!m.alpha_test() && !m.translucent(), "opaque images need no alpha flag");
        let fence = hp_vmt::parse(&std::fs::read_to_string(dest.join("fence.vmt")).unwrap_or_default()).unwrap();
        assert!(fence.alpha_test() && !fence.translucent(), "binary alpha is a cut out");
        assert_eq!(r[4].as_deref(), Ok("custom/test/glass"));
        let glass = hp_vmt::parse(&std::fs::read_to_string(dest.join("glass.vmt")).unwrap_or_default()).unwrap();
        assert!(glass.translucent() && !glass.alpha_test(), "partial alpha blends like custom folder images");

        let again = import_files(&files[..1], &opts);
        assert!(again[0].is_err());
        let over = import_files(&files[..1], &ImportOptions { overwrite: true, ..opts.clone() });
        assert!(over[0].is_ok());
        let dup =
            import_files(&[src.join("fence.png"), src.join("fence.png")], &ImportOptions { overwrite: true, ..opts });
        assert!(dup[0].is_ok() && dup[1].is_err());
        assert!(!dest.join("fence.vtf.tmp").exists());
    }
}
