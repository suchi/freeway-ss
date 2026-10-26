//! `cargo xtask icon`: generates `crates/freeway-scr/resources/freeway.ico`.
//!
//! The picture is a night road seen from a car: a dark navy square, a road
//! that narrows to a vanishing point on the horizon, and a row of lights along
//! each shoulder (orange on the left, white on the right) that shrink toward
//! the vanishing point like the delineators of the screensaver. A faint
//! orange glow sits on the horizon and a few stars in the sky.
//!
//! Every size is drawn from the same description (not scaled from one bitmap),
//! with fewer and relatively larger lights at small sizes so that they stay
//! readable. Drawing uses only plain arithmetic and `sqrt`, which are exact
//! and the same on every platform, so the output is identical everywhere and
//! a test can compare it with the checked-in file.
//!
//! Format: a 32-bit BMP image for each size up to 48 px and a PNG for 256 px,
//! the usual choice for icons. Both are written uncompressed: the encoders
//! stay tiny and need no dependency, and the 256 px image dominates the size
//! (about 0.3 MB).

use std::fs;
use std::path::Path;

use crate::crc32::crc32;

const SIZES: [u32; 4] = [16, 32, 48, 256];
const OUTPUT: &str = "crates/freeway-scr/resources/freeway.ico";

/// Samples per pixel along each axis, for anti-aliasing.
const SUPERSAMPLE: u32 = 4;

// Geometry in units of the icon size.
const MARGIN: f64 = 0.04;
const CORNER_RADIUS: f64 = 0.18;
const HORIZON: f64 = 0.38;
/// The vertical position of the nearest light.
const NEAREST: f64 = 0.88;
const ROAD_HALF_WIDTH: f64 = 0.36;
const LIGHT_RADIUS: f64 = 0.062;
const FIRST_LIGHT: f64 = 0.92;
const LIGHT_SPACING: f64 = 0.75;

type Rgb = [f64; 3];

const SKY_TOP: Rgb = [2.0 / 255.0, 3.0 / 255.0, 10.0 / 255.0];
const SKY_HORIZON: Rgb = [16.0 / 255.0, 30.0 / 255.0, 68.0 / 255.0];
const GROUND: Rgb = [3.0 / 255.0, 5.0 / 255.0, 12.0 / 255.0];
const ROAD: Rgb = [18.0 / 255.0, 26.0 / 255.0, 52.0 / 255.0];
const LEFT_LIGHT: Rgb = [1.0, 0.54, 0.12];
const RIGHT_LIGHT: Rgb = [1.0, 0.95, 0.82];
const STARS: [(f64, f64); 5] = [
    (0.2, 0.12),
    (0.75, 0.08),
    (0.62, 0.22),
    (0.33, 0.25),
    (0.88, 0.2),
];

pub fn run(root: &Path) -> Result<(), String> {
    let path = root.join(OUTPUT);
    fs::write(&path, build_ico()).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

/// Number of lights along each shoulder.
fn light_count(size: u32) -> usize {
    match size {
        0..=16 => 3,
        17..=32 => 4,
        33..=48 => 5,
        _ => 7,
    }
}

/// Renders one size as straight (not premultiplied) RGBA, top row first.
fn render(size: u32) -> Vec<u8> {
    let lights = lights(size);
    let mut pixels = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut sum = [0.0f64; 3];
            let mut covered = 0.0;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let x = (f64::from(px) + (f64::from(sx) + 0.5) / f64::from(SUPERSAMPLE))
                        / f64::from(size);
                    let y = (f64::from(py) + (f64::from(sy) + 0.5) / f64::from(SUPERSAMPLE))
                        / f64::from(size);
                    if inside_icon(x, y) {
                        let color = shade(x, y, size, &lights);
                        for channel in 0..3 {
                            sum[channel] += color[channel];
                        }
                        covered += 1.0;
                    }
                }
            }
            let total = f64::from(SUPERSAMPLE * SUPERSAMPLE);
            if covered == 0.0 {
                pixels.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                for channel in sum {
                    pixels.push(to_byte(channel / covered));
                }
                pixels.push(to_byte(covered / total));
            }
        }
    }
    pixels
}

fn to_byte(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

struct Light {
    x: f64,
    y: f64,
    radius: f64,
    color: Rgb,
}

fn lights(size: u32) -> Vec<Light> {
    let mut lights = Vec::new();
    for i in 0..light_count(size) {
        let depth = FIRST_LIGHT / (1.0 + LIGHT_SPACING * i as f64);
        let y = HORIZON + (NEAREST - HORIZON) * depth;
        // The road narrows linearly toward the horizon, so does the offset.
        let offset = ROAD_HALF_WIDTH * depth * 1.05;
        // A light never gets smaller than about a pixel, or it would vanish.
        let radius = (LIGHT_RADIUS * depth).max(0.7 / f64::from(size));
        lights.push(Light {
            x: 0.5 - offset,
            y,
            radius,
            color: LEFT_LIGHT,
        });
        lights.push(Light {
            x: 0.5 + offset,
            y,
            radius,
            color: RIGHT_LIGHT,
        });
    }
    lights
}

/// Whether a point is inside the rounded square of the icon.
fn inside_icon(x: f64, y: f64) -> bool {
    let (low, high) = (MARGIN, 1.0 - MARGIN);
    if x < low || x > high || y < low || y > high {
        return false;
    }
    // Distance into the corner square, if the point is in one.
    let dx = (low + CORNER_RADIUS - x)
        .max(x - (high - CORNER_RADIUS))
        .max(0.0);
    let dy = (low + CORNER_RADIUS - y)
        .max(y - (high - CORNER_RADIUS))
        .max(0.0);
    dx * dx + dy * dy <= CORNER_RADIUS * CORNER_RADIUS
}

fn shade(x: f64, y: f64, size: u32, lights: &[Light]) -> Rgb {
    let mut color = if y < HORIZON {
        // Darkest at the top, lighter toward the horizon.
        let t = (y / HORIZON).clamp(0.0, 1.0);
        mix(SKY_TOP, SKY_HORIZON, t * t)
    } else {
        let half_width = ROAD_HALF_WIDTH * (y - HORIZON) / (NEAREST - HORIZON);
        if (x - 0.5).abs() <= half_width {
            ROAD
        } else {
            GROUND
        }
    };

    // A warm glow where the road meets the sky.
    let glow = falloff(distance(x, y, 0.5, HORIZON), 0.28);
    color = add(color, [1.0, 0.55, 0.16], 0.22 * glow);

    if size >= 32 {
        for (sx, sy) in STARS {
            let star = falloff(distance(x, y, sx, sy), 0.02);
            color = add(color, [0.85, 0.9, 1.0], 0.9 * star);
        }
    }

    for light in lights {
        let d = distance(x, y, light.x, light.y);
        color = add(color, light.color, 0.55 * falloff(d, light.radius * 3.2));
        if d < light.radius {
            color = mix(color, mix(light.color, [1.0, 1.0, 1.0], 0.4), 0.95);
        }
    }
    color
}

fn distance(x: f64, y: f64, cx: f64, cy: f64) -> f64 {
    ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt()
}

/// 1 at the center, smoothly 0 at `radius` and beyond.
fn falloff(distance: f64, radius: f64) -> f64 {
    let t = (1.0 - (distance / radius) * (distance / radius)).max(0.0);
    t * t
}

fn mix(a: Rgb, b: Rgb, t: f64) -> Rgb {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn add(base: Rgb, color: Rgb, amount: f64) -> Rgb {
    [
        (base[0] + color[0] * amount).min(1.0),
        (base[1] + color[1] * amount).min(1.0),
        (base[2] + color[2] * amount).min(1.0),
    ]
}

/// The image data of one icon entry: a PNG for 256 px, a BMP otherwise.
fn encode(size: u32, rgba: &[u8]) -> Vec<u8> {
    if size >= 256 {
        encode_png(size, rgba)
    } else {
        encode_bmp(size, rgba)
    }
}

/// A DIB as stored in an ICO: a header, bottom-up BGRA rows, and an AND mask
/// (all zero, since the alpha channel carries the shape).
fn encode_bmp(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mask_row = size.div_ceil(32) * 4;
    let image_size = size * size * 4 + mask_row * size;
    let mut out = Vec::new();
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&(size * 2).to_le_bytes()); // XOR image plus AND mask
    out.extend_from_slice(&1u16.to_le_bytes()); // planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&0u32.to_le_bytes()); // BI_RGB
    out.extend_from_slice(&image_size.to_le_bytes());
    out.extend_from_slice(&[0; 16]); // resolution, palette sizes
    let row_bytes = size as usize * 4;
    for row in (0..size as usize).rev() {
        for pixel in rgba[row * row_bytes..(row + 1) * row_bytes]
            .as_chunks::<4>()
            .0
        {
            out.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    out.resize(out.len() + (mask_row * size) as usize, 0);
    out
}

fn encode_png(size: u32, rgba: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity((size * (size * 4 + 1)) as usize);
    for row in rgba.chunks_exact(size as usize * 4) {
        raw.push(0); // filter type: none
        raw.extend_from_slice(row);
    }
    // zlib stream of stored (uncompressed) deflate blocks.
    let mut zlib = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(block) = blocks.next() {
        zlib.push(u8::from(blocks.peek().is_none())); // BFINAL, BTYPE = stored
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut header = Vec::new();
    header.extend_from_slice(&size.to_be_bytes());
    header.extend_from_slice(&size.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA, no interlace

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    png_chunk(&mut out, b"IHDR", &header);
    png_chunk(&mut out, b"IDAT", &zlib);
    png_chunk(&mut out, b"IEND", &[]);
    out
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &byte in data {
        a = (a + u32::from(byte)) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn build_ico() -> Vec<u8> {
    let images: Vec<Vec<u8>> = SIZES
        .iter()
        .map(|&size| encode(size, &render(size)))
        .collect();
    let mut out = Vec::new();
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    out.extend_from_slice(&(SIZES.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * SIZES.len() as u32;
    for (&size, image) in SIZES.iter().zip(&images) {
        // 0 stands for 256.
        let dimension = if size >= 256 { 0 } else { size as u8 };
        out.extend_from_slice(&[dimension, dimension, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes()); // planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&(image.len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += image.len() as u32;
    }
    for image in &images {
        out.extend_from_slice(image);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(rgba: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * size + x) * 4) as usize;
        [rgba[at], rgba[at + 1], rgba[at + 2], rgba[at + 3]]
    }

    #[test]
    fn corners_are_transparent_and_the_center_is_opaque() {
        for size in SIZES {
            let image = render(size);
            assert_eq!(pixel(&image, size, 0, 0)[3], 0, "size {size}");
            assert_eq!(pixel(&image, size, size - 1, size - 1)[3], 0, "size {size}");
            assert_eq!(
                pixel(&image, size, size / 2, size / 2)[3],
                255,
                "size {size}"
            );
        }
    }

    #[test]
    fn the_sky_is_dark_and_the_lights_are_bright() {
        let size = 256;
        let image = render(size);
        let sky = pixel(&image, size, 128, 20);
        assert!(sky[0] < 40 && sky[1] < 40 && sky[2] < 60, "{sky:?}");
        // The nearest light on each shoulder.
        let left = lights(size)[0].x;
        let y = (lights(size)[0].y * f64::from(size)) as u32;
        let left_pixel = pixel(&image, size, (left * f64::from(size)) as u32, y);
        assert!(left_pixel[0] > 200, "{left_pixel:?}");
    }

    #[test]
    fn rendering_is_deterministic() {
        assert_eq!(render(32), render(32));
    }

    #[test]
    fn ico_directory_is_consistent() {
        let ico = build_ico();
        assert_eq!(&ico[..6], &[0, 0, 1, 0, 4, 0]);
        let mut end_of_images = 6 + 16 * 4;
        for (i, size) in SIZES.iter().enumerate() {
            let entry = &ico[6 + 16 * i..6 + 16 * (i + 1)];
            let dimension = if *size >= 256 { 0 } else { *size as u8 };
            assert_eq!(entry[0], dimension);
            assert_eq!(entry[1], dimension);
            assert_eq!(u16::from_le_bytes([entry[6], entry[7]]), 32);
            let length = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
            let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
            assert_eq!(offset, end_of_images);
            end_of_images += length;
            let image = &ico[offset..offset + length];
            if *size >= 256 {
                assert_eq!(&image[..8], b"\x89PNG\r\n\x1a\n");
            } else {
                // BITMAPINFOHEADER: size 40, width, doubled height.
                assert_eq!(u32::from_le_bytes(image[..4].try_into().unwrap()), 40);
                assert_eq!(u32::from_le_bytes(image[4..8].try_into().unwrap()), *size);
                assert_eq!(
                    u32::from_le_bytes(image[8..12].try_into().unwrap()),
                    size * 2
                );
            }
        }
        assert_eq!(end_of_images, ico.len());
    }

    #[test]
    fn png_chunks_have_valid_checksums_and_data() {
        let size = 256;
        let rgba = render(size);
        let png = encode_png(size, &rgba);
        let mut at = 8;
        let mut idat = Vec::new();
        let mut kinds = Vec::new();
        while at < png.len() {
            let length = u32::from_be_bytes(png[at..at + 4].try_into().unwrap()) as usize;
            let body = &png[at + 4..at + 8 + length];
            let crc =
                u32::from_be_bytes(png[at + 8 + length..at + 12 + length].try_into().unwrap());
            assert_eq!(crc32(body), crc);
            kinds.push(body[..4].to_vec());
            if &body[..4] == b"IDAT" {
                idat.extend_from_slice(&body[4..]);
            }
            at += 12 + length;
        }
        assert_eq!(
            kinds,
            [b"IHDR".to_vec(), b"IDAT".to_vec(), b"IEND".to_vec()]
        );
        // Undo the stored deflate blocks and compare with the image.
        let mut raw = Vec::new();
        let mut at = 2;
        loop {
            let last = idat[at] & 1 == 1;
            let length = u16::from_le_bytes([idat[at + 1], idat[at + 2]]) as usize;
            let complement = u16::from_le_bytes([idat[at + 3], idat[at + 4]]);
            assert_eq!(complement, !(length as u16));
            raw.extend_from_slice(&idat[at + 5..at + 5 + length]);
            at += 5 + length;
            if last {
                break;
            }
        }
        assert_eq!(
            u32::from_be_bytes(idat[at..at + 4].try_into().unwrap()),
            adler32(&raw)
        );
        let rows: Vec<u8> = raw
            .chunks_exact(size as usize * 4 + 1)
            .flat_map(|row| {
                assert_eq!(row[0], 0);
                row[1..].to_vec()
            })
            .collect();
        assert_eq!(rows, rgba);
    }

    #[test]
    fn adler32_matches_the_reference_value() {
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
    }

    #[test]
    fn checked_in_icon_is_up_to_date() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(OUTPUT);
        let checked_in = fs::read(&path).expect("run `cargo xtask icon` to generate the icon");
        assert!(
            checked_in == build_ico(),
            "{OUTPUT} is stale; run `cargo xtask icon`"
        );
    }
}
