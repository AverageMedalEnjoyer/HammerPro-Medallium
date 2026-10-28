use super::*;

struct Synth {
    minor: u32,
    format: ImageFormat,
    width: u32,
    height: u32,
    depth: u32,
    frames: u32,
    faces: u32,
    mips: u32,
    flags: u32,
    first_frame: u16,
    low_res: Option<(u32, u32)>,
}

impl Default for Synth {
    fn default() -> Self {
        Self {
            minor: 2,
            format: ImageFormat::Rgba8888,
            width: 8,
            height: 4,
            depth: 1,
            frames: 1,
            faces: 1,
            mips: 1,
            flags: 0,
            first_frame: 0,
            low_res: None,
        }
    }
}

fn tag(mip: u32, frame: u32, face: u32, slice: u32) -> u8 {
    (mip * 64 + frame * 16 + face * 2 + slice) as u8
}

fn build(s: &Synth) -> Vec<u8> {
    let mut data = Vec::new();
    for mip in (0..s.mips).rev() {
        let (w, h, d) = (header::shrink(s.width, mip), header::shrink(s.height, mip), header::shrink(s.depth, mip));
        let size = s.format.image_size(w, h).unwrap() as usize;
        for frame in 0..s.frames {
            for face in 0..s.faces {
                for slice in 0..d {
                    data.extend(std::iter::repeat_n(tag(mip, frame, face, slice), size));
                }
            }
        }
    }
    let low =
        s.low_res.map(|(w, h)| vec![0xEE; ImageFormat::Dxt1.image_size(w, h).unwrap() as usize]).unwrap_or_default();
    let header_size: usize = match s.minor {
        0 | 1 => 64,
        2 => 80,
        _ => 104,
    };
    let mut out = Vec::new();
    out.extend_from_slice(b"VTF\0");
    out.extend_from_slice(&7u32.to_le_bytes());
    out.extend_from_slice(&s.minor.to_le_bytes());
    out.extend_from_slice(&(header_size as u32).to_le_bytes());
    out.extend_from_slice(&(s.width as u16).to_le_bytes());
    out.extend_from_slice(&(s.height as u16).to_le_bytes());
    out.extend_from_slice(&s.flags.to_le_bytes());
    out.extend_from_slice(&(s.frames as u16).to_le_bytes());
    out.extend_from_slice(&s.first_frame.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    for v in [0.25f32, 0.5, 0.75, 0.0, 2.0] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&s.format.raw().to_le_bytes());
    out.push(s.mips as u8);
    match s.low_res {
        Some((w, h)) => {
            out.extend_from_slice(&ImageFormat::Dxt1.raw().to_le_bytes());
            out.push(w as u8);
            out.push(h as u8);
        }
        None => {
            out.extend_from_slice(&u32::MAX.to_le_bytes());
            out.extend_from_slice(&[0, 0]);
        }
    }
    if s.minor >= 2 {
        out.extend_from_slice(&(s.depth as u16).to_le_bytes());
    }
    if s.minor >= 3 {
        out.extend_from_slice(&[0; 3]);
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(b"CRC");
        out.extend_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
        out.extend_from_slice(&[0x30, 0, 0, 0]);
        out.extend_from_slice(&((header_size + low.len()) as u32).to_le_bytes());
        out.extend_from_slice(&[0x01, 0, 0, 0]);
        out.extend_from_slice(&(header_size as u32).to_le_bytes());
    }
    assert!(out.len() <= header_size);
    out.resize(header_size, 0);
    out.extend_from_slice(&low);
    out.extend_from_slice(&data);
    out
}

fn decode_tag(data: &[u8], mip: u32, frame: u32, face: u32) -> u8 {
    let img = decode_rgba8(data, &DecodeOptions { mip: MipSelect::Index(mip), frame, face }).unwrap();
    img.pixels[0]
}

#[test]
fn layout_mips_frames_faces_slices_all_versions() {
    for minor in 0..=5 {
        let s = Synth {
            minor,
            width: 16,
            height: 8,
            depth: if minor >= 2 { 4 } else { 1 },
            frames: 3,
            mips: 5,
            low_res: Some((16, 8)),
            ..Synth::default()
        };
        let data = build(&s);
        let info = read_info(&data).unwrap();
        assert_eq!(info.version, (7, minor));
        assert_eq!((info.width, info.height, info.depth), (16, 8, s.depth));
        assert_eq!((info.frames, info.faces, info.mip_count), (3, 1, 5));
        assert_eq!(info.reflectivity, [0.25, 0.5, 0.75]);
        assert_eq!(info.bumpmap_scale, 2.0);
        assert_eq!(info.low_res_format, Some(ImageFormat::Dxt1));
        for mip in 0..5 {
            for frame in 0..3 {
                assert_eq!(decode_tag(&data, mip, frame, 0), tag(mip, frame, 0, 0));
            }
            let lvl = raw_level(&data, mip, 2, 0).unwrap();
            assert_eq!(lvl.width, (16 >> mip).max(1));
            assert!(lvl.data.iter().all(|&b| b == tag(mip, 2, 0, 0)));
        }
        assert_eq!(raw_mips(&data, 1, 0).unwrap().len(), 5);
    }
}

#[test]
fn cubemap_face_counts() {
    let s = Synth { minor: 5, faces: 6, mips: 3, flags: flags::ENVMAP, ..Synth::default() };
    let data = build(&s);
    let info = read_info(&data).unwrap();
    assert_eq!(info.faces, 6);
    assert!(info.is_cubemap());
    for face in 0..6 {
        assert_eq!(decode_tag(&data, 0, 0, face), tag(0, 0, face, 0));
    }
    assert!(matches!(
        decode_rgba8(&data, &DecodeOptions { face: 6, ..DecodeOptions::default() }),
        Err(VtfError::FaceOutOfRange { index: 6, count: 6 })
    ));

    let s = Synth { minor: 4, faces: 7, mips: 3, flags: flags::ENVMAP, ..Synth::default() };
    let data = build(&s);
    assert_eq!(read_info(&data).unwrap().faces, 7);
    for face in 0..7 {
        assert_eq!(decode_tag(&data, 1, 0, face), tag(1, 0, face, 0));
    }

    let s = Synth { minor: 4, faces: 6, mips: 2, flags: flags::ENVMAP, first_frame: 0xFFFF, ..Synth::default() };
    assert_eq!(read_info(&build(&s)).unwrap().faces, 6);

    let s = Synth { minor: 2, faces: 6, mips: 2, flags: flags::ENVMAP, ..Synth::default() };
    let data = build(&s);
    assert_eq!(read_info(&data).unwrap().faces, 6);
    assert_eq!(decode_tag(&data, 0, 0, 5), tag(0, 0, 5, 0));
}

#[test]
fn block_formats_with_tiny_mips() {
    for format in [ImageFormat::Dxt1, ImageFormat::Dxt5, ImageFormat::Ati2n] {
        let s = Synth { format, width: 8, height: 2, mips: 4, ..Synth::default() };
        let data = build(&s);
        for mip in 0..4 {
            let lvl = raw_level(&data, mip, 0, 0).unwrap();
            assert_eq!(lvl.data.len() as u32, format.bc_format().unwrap().block_bytes() * (lvl.width.div_ceil(4)));
            let img = lvl.to_rgba8().unwrap();
            assert_eq!(img.pixels.len() as u32, lvl.width * lvl.height * 4);
        }
    }
}

fn gradient(w: u32, h: u32, alpha: bool) -> RgbaImage {
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let a = if alpha { ((x * 255) / w.max(2).saturating_sub(1).max(1)).min(255) as u8 } else { 255 };
            pixels.extend_from_slice(&[
                (x * 255 / w.max(1)) as u8,
                (y * 255 / h.max(1)) as u8,
                ((x + y) * 127 / (w + h)) as u8,
                a,
            ]);
        }
    }
    RgbaImage::new(w, h, pixels).unwrap()
}

fn mean_abs_error(a: &[u8], b: &[u8]) -> f64 {
    assert_eq!(a.len(), b.len());
    let sum: u64 = a.iter().zip(b).map(|(x, y)| u64::from(x.abs_diff(*y))).sum();
    sum as f64 / a.len() as f64
}

#[test]
fn round_trip_every_encode_format() {
    for format in
        [EncodeFormat::Auto, EncodeFormat::Dxt1, EncodeFormat::Dxt5, EncodeFormat::Bgra8888, EncodeFormat::Bgr888]
    {
        for mipmaps in [false, true] {
            for alpha in [false, true] {
                let img = gradient(64, 32, alpha);
                let opts = EncodeOptions { format, mipmaps, ..EncodeOptions::default() };
                let vtf = encode(&img, &opts).unwrap();
                let info = read_info(&vtf).unwrap();
                let expected_format = match format {
                    EncodeFormat::Auto if alpha => ImageFormat::Dxt5,
                    EncodeFormat::Auto | EncodeFormat::Dxt1 => ImageFormat::Dxt1,
                    EncodeFormat::Dxt5 => ImageFormat::Dxt5,
                    EncodeFormat::Bgra8888 => ImageFormat::Bgra8888,
                    EncodeFormat::Bgr888 => ImageFormat::Bgr888,
                };
                let ctx = format!("{format:?} mips={mipmaps} alpha={alpha}");
                assert_eq!(info.format, expected_format, "{ctx}");
                assert_eq!(info.mip_count, if mipmaps { 7 } else { 1 }, "{ctx}");
                assert_eq!((info.width, info.height), (64, 32), "{ctx}");
                assert_eq!((info.low_res_width, info.low_res_height), (16, 8), "{ctx}");
                assert_eq!(info.version, (7, 4));
                let last = raw_level(&vtf, 0, 0, 0).unwrap();
                assert_eq!(last.data.as_ptr_range().end, vtf.as_ptr_range().end, "{ctx}");

                let out = decode_rgba8(&vtf, &DecodeOptions::default()).unwrap();
                assert_eq!((out.width, out.height), (64, 32));
                let rgb = |p: &[u8]| -> Vec<u8> { p.chunks_exact(4).flat_map(|c| [c[0], c[1], c[2]]).collect() };
                let alpha_of = |p: &[u8]| -> Vec<u8> { p.chunks_exact(4).map(|c| c[3]).collect() };
                let err = mean_abs_error(&rgb(&img.pixels), &rgb(&out.pixels));
                match expected_format {
                    ImageFormat::Bgra8888 | ImageFormat::Bgr888 => {
                        assert_eq!(err, 0.0, "{ctx}");
                    }
                    ImageFormat::Dxt1 if alpha => {}
                    _ => assert!(err < 3.0, "{ctx}: color error {err}"),
                }
                let a_err = mean_abs_error(&alpha_of(&img.pixels), &alpha_of(&out.pixels));
                match (expected_format, alpha) {
                    (ImageFormat::Bgr888, true) => assert!(out.is_opaque()),
                    (ImageFormat::Dxt1, true) => {
                        assert!(info.has_flag(flags::ONEBITALPHA), "{ctx}");
                        assert!(out.pixels.chunks_exact(4).all(|p| p[3] == 0 || p[3] == 255));
                        assert!(!out.is_opaque());
                    }
                    (ImageFormat::Bgra8888, _) => assert_eq!(a_err, 0.0, "{ctx}"),
                    (ImageFormat::Dxt5, true) => {
                        assert!(a_err < 2.0, "{ctx}: alpha error {a_err}");
                        assert!(info.has_flag(flags::EIGHTBITALPHA), "{ctx}");
                    }
                    _ => {}
                }
                if !alpha {
                    assert!(out.is_opaque(), "{ctx}");
                    assert!(!info.has_flag(flags::EIGHTBITALPHA | flags::ONEBITALPHA));
                }
                if mipmaps {
                    for mip in 0..info.mip_count {
                        let m = decode_rgba8(
                            &vtf,
                            &DecodeOptions { mip: MipSelect::Index(mip), ..DecodeOptions::default() },
                        )
                        .unwrap();
                        assert_eq!(m.width, (64 >> mip).max(1));
                        assert_eq!(m.height, (32 >> mip).max(1));
                    }
                }
                let low = decode_low_res_rgba8(&vtf).unwrap().unwrap();
                assert_eq!((low.width, low.height), (16, 8));
                assert!(low.is_opaque());
            }
        }
    }
}

#[test]
fn every_version_round_trips() {
    let img = gradient(32, 32, true);
    let reference = decode_rgba8(&encode(&img, &EncodeOptions::default()).unwrap(), &DecodeOptions::default()).unwrap();
    for minor in 0..=5 {
        let vtf = encode(&img, &EncodeOptions { version_minor: minor, ..EncodeOptions::default() }).unwrap();
        let info = read_info(&vtf).unwrap();
        assert_eq!(info.version, (7, minor));
        assert_eq!(
            info.header_size,
            if minor < 2 {
                64
            } else if minor == 2 {
                80
            } else {
                96
            }
        );
        let out = decode_rgba8(&vtf, &DecodeOptions::default()).unwrap();
        assert_eq!(out, reference, "version 7.{minor}");
        assert!(decode_low_res_rgba8(&vtf).unwrap().is_some());
    }
    assert!(matches!(
        encode(&img, &EncodeOptions { version_minor: 6, ..EncodeOptions::default() }),
        Err(VtfError::UnsupportedVersion(7, 6))
    ));
}

#[test]
fn solid_color_survives_every_mip() {
    let img = RgbaImage::new(128, 128, [200, 100, 50, 255].repeat(128 * 128)).unwrap();
    let vtf = encode(&img, &EncodeOptions { format: EncodeFormat::Bgr888, ..EncodeOptions::default() }).unwrap();
    let info = read_info(&vtf).unwrap();
    assert_eq!(info.mip_count, 8);
    let r = info.reflectivity;
    let expected = (200.0f32 / 255.0).powf(2.2);
    assert!((r[0] - expected).abs() < 1e-4, "{r:?}");
    for mip in 0..8 {
        let lvl =
            decode_rgba8(&vtf, &DecodeOptions { mip: MipSelect::Index(mip), ..DecodeOptions::default() }).unwrap();
        assert!(lvl.pixels.chunks_exact(4).all(|p| p == [200, 100, 50, 255]), "mip {mip}");
    }
}

#[test]
fn resize_to_pow2_and_npot() {
    let img = gradient(300, 200, false);
    let vtf = encode(&img, &EncodeOptions::default()).unwrap();
    let info = read_info(&vtf).unwrap();
    assert_eq!((info.width, info.height), (256, 256));
    assert_eq!(info.mip_count, 9);

    let npot = encode(
        &img,
        &EncodeOptions { resize_to_pow2: false, format: EncodeFormat::Bgra8888, ..EncodeOptions::default() },
    )
    .unwrap();
    let info = read_info(&npot).unwrap();
    assert_eq!((info.width, info.height), (300, 200));
    assert_eq!(info.mip_count, 9);
    let out = decode_rgba8(&npot, &DecodeOptions::default()).unwrap();
    assert_eq!(out.pixels, img.pixels);
    let smallest = raw_level(&npot, 8, 0, 0).unwrap();
    assert_eq!((smallest.width, smallest.height), (1, 1));
}

#[test]
fn thumbnails_pick_small_mips_or_downscale() {
    let img = gradient(512, 256, false);
    let vtf = encode(&img, &EncodeOptions::default()).unwrap();
    let thumb = decode_rgba8(&vtf, &DecodeOptions::thumbnail(128)).unwrap();
    assert_eq!((thumb.width, thumb.height), (128, 64));
    let thumb = decode_rgba8(&vtf, &DecodeOptions::thumbnail(100)).unwrap();
    assert_eq!((thumb.width, thumb.height), (64, 32));

    let nomip = encode(&img, &EncodeOptions { mipmaps: false, ..EncodeOptions::default() }).unwrap();
    let thumb = decode_rgba8(&nomip, &DecodeOptions::thumbnail(128)).unwrap();
    assert_eq!((thumb.width, thumb.height), (128, 64));
    let thumb = decode_rgba8(&nomip, &DecodeOptions::thumbnail(0)).unwrap();
    assert_eq!((thumb.width, thumb.height), (1, 1));
}

#[test]
fn prefix_is_enough_for_thumbnails() {
    let img = gradient(1024, 1024, true);
    let vtf = encode(&img, &EncodeOptions { quality: EncodeQuality::Fast, ..EncodeOptions::default() }).unwrap();
    let opts = DecodeOptions::thumbnail(128);
    let needed = required_len(&vtf[..MAX_HEADER_LEN.min(vtf.len())], &opts).unwrap();
    assert!(needed < vtf.len() / 20, "{needed} of {}", vtf.len());
    let from_prefix = decode_rgba8(&vtf[..needed], &opts).unwrap();
    let from_full = decode_rgba8(&vtf, &opts).unwrap();
    assert_eq!(from_prefix, from_full);
    assert!(matches!(decode_rgba8(&vtf[..needed - 1], &opts), Err(VtfError::Truncated { .. })));
    assert!(matches!(required_len(&vtf[..40], &opts), Err(VtfError::Truncated { needed: 80, .. })));
    assert_eq!(required_len(&vtf, &DecodeOptions::default()).unwrap(), vtf.len());
}

#[test]
fn rejects_bad_headers() {
    assert_eq!(read_info(b"NOPE1234567890123456"), Err(VtfError::BadSignature));
    assert!(matches!(read_info(b"VTF\0"), Err(VtfError::Truncated { .. })));
    assert!(matches!(read_info(&[]), Err(VtfError::Truncated { .. })));
    let mut data = build(&Synth::default());
    assert!(is_vtf(&data));
    data[4] = 8;
    assert_eq!(read_info(&data), Err(VtfError::UnsupportedVersion(8, 2)));

    let mut data = build(&Synth::default());
    data[52] = 99;
    assert_eq!(read_info(&data), Err(VtfError::UnknownFormat(99)));

    let mut data = build(&Synth::default());
    data[16] = 0;
    data[17] = 0;
    assert!(matches!(read_info(&data), Err(VtfError::InvalidDimensions { .. })));

    let data = build(&Synth { format: ImageFormat::NvIntz, ..Synth::default() });
    assert!(read_info(&data).is_ok());
    assert_eq!(decode_rgba8(&data, &DecodeOptions::default()), Err(VtfError::UnsupportedFormat(ImageFormat::NvIntz)));
    let data = build(&Synth::default());
    assert!(matches!(
        decode_rgba8(&data, &DecodeOptions { mip: MipSelect::Index(1), ..DecodeOptions::default() }),
        Err(VtfError::MipOutOfRange { index: 1, count: 1 })
    ));
    assert!(matches!(raw_level(&data, 0, 1, 0), Err(VtfError::FrameOutOfRange { index: 1, count: 1 })));
}

#[test]
fn huge_declared_sizes_do_not_overflow_or_allocate() {
    let mut data = build(&Synth { minor: 2, ..Synth::default() });
    data[16..20].copy_from_slice(&[0xFF; 4]);
    data[24..26].copy_from_slice(&[0xFF; 2]);
    data[52..56].copy_from_slice(&24u32.to_le_bytes());
    data[56] = 255;
    data[63..65].copy_from_slice(&[0xFF; 2]);
    assert_eq!(read_info(&data), Err(VtfError::TooLarge));
    data[24..26].copy_from_slice(&[1, 0]);
    data[63..65].copy_from_slice(&[1, 0]);
    let info = read_info(&data).unwrap();
    assert_eq!(info.mip_count, 255);
    assert!(matches!(decode_rgba8(&data, &DecodeOptions::default()), Err(VtfError::Truncated { .. })));
    assert_eq!(info.mip_dimensions(200), (1, 1, 1));
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }
}

#[test]
fn hostile_input_never_panics() {
    let mut samples = Vec::new();
    for minor in [1, 2, 4] {
        samples.push(
            encode(
                &gradient(16, 8, true),
                &EncodeOptions { version_minor: minor, quality: EncodeQuality::Fast, ..EncodeOptions::default() },
            )
            .unwrap(),
        );
    }
    samples.push(build(&Synth { minor: 5, faces: 6, mips: 3, frames: 2, flags: flags::ENVMAP, ..Synth::default() }));
    let exercise = |d: &[u8]| {
        let _ = read_info(d);
        let _ = decode_rgba8(d, &DecodeOptions::default());
        let _ = decode_rgba8(d, &DecodeOptions::thumbnail(4));
        let _ = raw_mips(d, 0, 0);
        let _ = decode_low_res_rgba8(d);
        let _ = required_len(d, &DecodeOptions::default());
    };
    let mut rng = Lcg(0x5EED);
    for sample in &samples {
        for len in 0..sample.len() {
            exercise(&sample[..len]);
        }
        for _ in 0..3000 {
            let mut d = sample.clone();
            for _ in 0..1 + rng.next() % 4 {
                let span = if rng.next().is_multiple_of(4) { d.len() } else { 120.min(d.len()) };
                let i = (rng.next() as usize) % span;
                d[i] = rng.next() as u8;
            }
            exercise(&d);
        }
    }
}

#[test]
fn load_image_files_in_every_supported_format() {
    use std::io::Cursor;
    let src = gradient(13, 7, true);
    let buf = image::RgbaImage::from_raw(13, 7, src.pixels.clone()).unwrap();
    for (fmt, ext, lossless) in [
        (image::ImageFormat::Png, "png", true),
        (image::ImageFormat::Tga, "tga", true),
        (image::ImageFormat::Bmp, "bmp", true),
        (image::ImageFormat::Jpeg, "jpg", false),
    ] {
        let mut bytes = Cursor::new(Vec::new());
        if fmt == image::ImageFormat::Jpeg {
            image::DynamicImage::ImageRgba8(buf.clone()).into_rgb8().write_to(&mut bytes, fmt).unwrap();
        } else {
            buf.write_to(&mut bytes, fmt).unwrap();
        }
        let bytes = bytes.into_inner();
        let img = load_image_file(&bytes, ext).unwrap();
        assert_eq!((img.width, img.height), (13, 7), "{ext}");
        if lossless {
            assert_eq!(img.pixels, src.pixels, "{ext}");
        }
        if fmt != image::ImageFormat::Tga {
            assert!(load_image_file(&bytes, "Texture.Name.TGA.bmp.png.jpg").is_ok());
        }
    }
    assert!(matches!(load_image_file(b"definitely not an image", "txt"), Err(VtfError::ImageDecode(_))));
    assert!(matches!(load_image_file(b"\x89PNG\r\n\x1a\nbroken", "png"), Err(VtfError::ImageDecode(_))));
}

#[test]
fn encoder_rejects_bad_input() {
    assert!(matches!(
        encode(&RgbaImage::default(), &EncodeOptions::default()),
        Err(VtfError::InvalidDimensions { .. })
    ));
    let bad = RgbaImage { width: 4, height: 4, pixels: vec![0; 3] };
    assert!(matches!(encode(&bad, &EncodeOptions::default()), Err(VtfError::InvalidImage(_))));
    assert!(RgbaImage::new(2, 2, vec![0; 15]).is_err());
    assert!(matches!(encode_frames(&[], &EncodeOptions::default()), Err(VtfError::InvalidImage(_))));
    let a = gradient(8, 8, false);
    let b = gradient(4, 4, false);
    assert!(matches!(encode_frames(&[a, b], &EncodeOptions::default()), Err(VtfError::InvalidImage(_))));
}

#[test]
fn animated_textures_round_trip() {
    let frames: Vec<RgbaImage> =
        (0..3u8).map(|i| RgbaImage::new(8, 8, [i * 80, 0, 0, 255].repeat(64)).unwrap()).collect();
    let vtf =
        encode_frames(&frames, &EncodeOptions { format: EncodeFormat::Bgra8888, ..EncodeOptions::default() }).unwrap();
    let info = read_info(&vtf).unwrap();
    assert_eq!(info.frames, 3);
    for (i, f) in frames.iter().enumerate() {
        for mip in 0..info.mip_count {
            let out =
                decode_rgba8(&vtf, &DecodeOptions { mip: MipSelect::Index(mip), frame: i as u32, face: 0 }).unwrap();
            assert_eq!(&out.pixels[..4], &f.pixels[..4], "frame {i} mip {mip}");
        }
    }
}

#[test]
fn flags_are_written_and_envmap_is_stripped() {
    let vtf = encode(
        &gradient(8, 8, false),
        &EncodeOptions { flags: flags::NORMAL | flags::CLAMPS | flags::ENVMAP, ..EncodeOptions::default() },
    )
    .unwrap();
    let info = read_info(&vtf).unwrap();
    assert_eq!(info.flags, flags::NORMAL | flags::CLAMPS);
    assert_eq!(info.faces, 1);
}
