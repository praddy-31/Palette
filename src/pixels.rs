//! Pixels in and out of image files. On macOS without an image library:
//! macOS's own `sips` turns anything it can open (PNG, JPEG, HEIC, WebP,
//! TIFF…) into an uncompressed BMP and back; this module reads and writes that
//! BMP, which is just a header and rows of bytes. Keeps the Mac binary small.
//! Linux has no `sips`, so there the `image` crate decodes and encodes PNG,
//! JPEG and WebP in-process (a Linux-only dependency, see Cargo.toml).

use std::fs;
use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;

use anyhow::{Context, Result, anyhow, bail};

#[cfg(target_os = "macos")]
use crate::system;

/// An image as rows of RGB pixels, top row first.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<[u8; 3]>,
}

impl Image {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            rgb: vec![[0; 3]; width * height],
        }
    }
}

/// Read any image macOS can open. `max_side`: shrink so the longer side is
/// at most this many pixels (enough for picking colours, much faster).
#[cfg(target_os = "macos")]
pub fn read(path: &Path, max_side: Option<u32>) -> Result<Image> {
    let tmp = temp_file("bmp");
    let (src, out) = (path.to_string_lossy(), tmp.to_string_lossy());
    let size = max_side.map(|s| s.to_string());
    let mut args = vec!["-s", "format", "bmp"];
    if let Some(size) = &size {
        args.extend(["-Z", size]);
    }
    args.extend([&*src, "--out", &*out]);
    let converted = system::run("sips", &args).map_err(|e| anyhow!(e));
    let bytes = converted.and_then(|_| fs::read(&tmp).context("reading the converted image"));
    let _ = fs::remove_file(&tmp);
    decode_bmp(&bytes.with_context(|| format!("opening {}", path.display()))?)
}

/// PNG or JPEG, by `path`'s extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Png,
    Jpeg,
}

fn format_of(path: &Path) -> Result<Format> {
    let ext = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => Ok(Format::Png),
        "jpg" | "jpeg" => Ok(Format::Jpeg),
        _ => bail!("{}: write .png or .jpg", path.display()),
    }
}

/// JPEG quality, both systems.
const JPEG_QUALITY: u8 = 92;

/// Write `image` as PNG or JPEG, by `path`'s extension.
#[cfg(target_os = "macos")]
pub fn write(path: &Path, image: &Image) -> Result<()> {
    let format = match format_of(path)? {
        Format::Png => "png",
        Format::Jpeg => "jpeg",
    };
    let quality = JPEG_QUALITY.to_string();
    let tmp = temp_file("bmp");
    fs::write(&tmp, encode_bmp(image)).context("writing a temporary image")?;
    let (src, out) = (tmp.to_string_lossy(), path.to_string_lossy());
    let mut args = vec!["-s", "format", format];
    if format == "jpeg" {
        args.extend(["-s", "formatOptions", &quality]);
    }
    args.extend([&*src, "--out", &*out]);
    let done = system::run("sips", &args).map_err(|e| anyhow!(e));
    let _ = fs::remove_file(&tmp);
    done.map(|_| ())
        .with_context(|| format!("saving {}", path.display()))
}

/// A JPEG of `src` at most `max_side` pixels on its longer side (the picker's
/// thumbnails). macOS: one `sips` run, which keeps HEIC and TIFF working.
#[cfg(target_os = "macos")]
pub fn thumbnail(src: &Path, dst: &Path, max_side: u32) -> Result<()> {
    let (src, dst, size) = (
        src.to_string_lossy(),
        dst.to_string_lossy(),
        max_side.to_string(),
    );
    system::run(
        "sips",
        &["-Z", &size, "-s", "format", "jpeg", &src, "--out", &dst],
    )
    .map(|_| ())
    .map_err(|e| anyhow!(e))
}

/// Read a PNG, JPEG or WebP. `max_side`: shrink so the longer side is at
/// most this many pixels.
#[cfg(not(target_os = "macos"))]
pub fn read(path: &Path, max_side: Option<u32>) -> Result<Image> {
    let mut picture = image::ImageReader::open(path)
        .with_context(|| format!("opening {}", path.display()))?
        .with_guessed_format()
        .with_context(|| format!("opening {}", path.display()))?
        .decode()
        .with_context(|| {
            format!(
                "{} isn't a PNG, JPEG or WebP Palette can read",
                path.display()
            )
        })?;
    if let Some(max) = max_side
        && picture.width().max(picture.height()) > max
    {
        picture = picture.resize(max, max, image::imageops::FilterType::Triangle);
    }
    let rgb = picture.to_rgb8();
    let (width, height) = (rgb.width() as usize, rgb.height() as usize);
    Ok(Image {
        width,
        height,
        rgb: rgb.pixels().map(|p| p.0).collect(),
    })
}

/// Write `image` as PNG or JPEG, by `path`'s extension.
#[cfg(not(target_os = "macos"))]
pub fn write(path: &Path, image: &Image) -> Result<()> {
    let format = format_of(path)?;
    let bytes: Vec<u8> = image.rgb.iter().flatten().copied().collect();
    let buffer = image::RgbImage::from_raw(image.width as u32, image.height as u32, bytes)
        .ok_or_else(|| anyhow!("image size doesn't match its pixels"))?;
    let saved = match format {
        Format::Png => buffer.save_with_format(path, image::ImageFormat::Png),
        Format::Jpeg => fs::File::create(path)
            .map_err(image::ImageError::IoError)
            .and_then(|file| {
                let mut out = std::io::BufWriter::new(file);
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
                    .encode_image(&buffer)
            }),
    };
    saved.with_context(|| format!("saving {}", path.display()))
}

/// A JPEG of `src` at most `max_side` pixels on its longer side.
#[cfg(not(target_os = "macos"))]
pub fn thumbnail(src: &Path, dst: &Path, max_side: u32) -> Result<()> {
    write(dst, &read(src, Some(max_side))?)
}

#[cfg(target_os = "macos")]
fn temp_file(ext: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("palette-{}-{nanos}.{ext}", std::process::id()))
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// 24-bit and 32-bit uncompressed BMPs (what `sips` writes), either row order.
pub fn decode_bmp(b: &[u8]) -> Result<Image> {
    let bad = || anyhow!("not a BMP Palette can read");
    if b.get(0..2) != Some(b"BM") {
        return Err(bad());
    }
    let offset = u32_at(b, 10).ok_or_else(bad)? as usize;
    let width = u32_at(b, 18).ok_or_else(bad)? as i32;
    let height = u32_at(b, 22).ok_or_else(bad)? as i32;
    let bpp = u16_at(b, 28).ok_or_else(bad)?;
    let compression = u32_at(b, 30).ok_or_else(bad)?;
    // 0: plain rows; 3: bit fields, which sips uses for 32-bit (BGRA order).
    if width <= 0 || height == 0 || !matches!((bpp, compression), (24, 0) | (32, 0) | (32, 3)) {
        bail!("unsupported BMP ({bpp}-bit, compression {compression})");
    }
    let (w, h) = (width as usize, height.unsigned_abs() as usize);
    if w * h > 100_000_000 {
        bail!("image too large ({w}×{h})");
    }
    let step = bpp as usize / 8;
    let stride = (w * step).div_ceil(4) * 4;
    let mut image = Image::new(w, h);
    for row in 0..h {
        // Positive height: bottom row first.
        let src_row = if height > 0 { h - 1 - row } else { row };
        let start = offset + src_row * stride;
        let line = b.get(start..start + w * step).ok_or_else(bad)?;
        for (x, px) in line.chunks_exact(step).enumerate() {
            image.rgb[row * w + x] = [px[2], px[1], px[0]];
        }
    }
    Ok(image)
}

/// A 24-bit, top-down BMP.
pub fn encode_bmp(image: &Image) -> Vec<u8> {
    let stride = (image.width * 3).div_ceil(4) * 4;
    let size = 54 + stride * image.height;
    let mut b = Vec::with_capacity(size);
    b.extend_from_slice(b"BM");
    b.extend_from_slice(&(size as u32).to_le_bytes());
    b.extend_from_slice(&[0; 4]);
    b.extend_from_slice(&54u32.to_le_bytes());
    b.extend_from_slice(&40u32.to_le_bytes());
    b.extend_from_slice(&(image.width as i32).to_le_bytes());
    b.extend_from_slice(&(-(image.height as i32)).to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&[0; 24]);
    for row in image.rgb.chunks_exact(image.width.max(1)) {
        for [r, g, bl] in row {
            b.extend_from_slice(&[*bl, *g, *r]);
        }
        b.resize(b.len() + stride - image.width * 3, 0);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bmp_round_trip() {
        let mut image = Image::new(3, 2);
        image.rgb = vec![
            [255, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [10, 20, 30],
            [40, 50, 60],
            [70, 80, 90],
        ];
        assert_eq!(decode_bmp(&encode_bmp(&image)).unwrap(), image);
    }

    /// Through `sips` on macOS, the `image` crate on Linux.
    #[test]
    fn png_round_trip_and_shrink() {
        let dir = tempfile::tempdir().unwrap();
        let mut image = Image::new(4, 4);
        image.rgb.iter_mut().enumerate().for_each(|(i, p)| {
            *p = [i as u8 * 16, 100, 200];
        });
        let png = dir.path().join("a.png");
        write(&png, &image).unwrap();
        assert_eq!(read(&png, None).unwrap(), image, "PNG is lossless");
        assert_eq!(read(&png, Some(2)).unwrap().width, 2);
    }
}
