//! Artwork effects: halftone dots, 1-bit ordered dither, and ASCII art.
//! Everything here runs on worker threads, on the bytes fetched for a cover.

use image::{RgbaImage, imageops::FilterType};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    /// Coloured halftone dots on black.
    Dots,
    /// Two-tone ordered (Bayer) dither in big crisp pixels.
    Dither,
    /// The cover as it is.
    Original,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Dots, Style::Dither, Style::Original];
    pub fn name(self) -> &'static str {
        match self {
            Style::Dots => "Dots",
            Style::Dither => "Dither",
            Style::Original => "Original",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Style::Dots => "Covers as coloured halftone dots, like print under a loupe.",
            Style::Dither => "Two-tone 1-bit dither in chunky pixels, like an old Mac screen.",
            Style::Original => "Covers exactly as the artist made them.",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Style::Dots => "dots",
            Style::Dither => "dither",
            Style::Original => "original",
        }
    }
    pub fn from_id(id: &str) -> Style {
        Style::ALL.into_iter().find(|s| s.id() == id).unwrap_or(Style::Dots)
    }
}

/// A processed cover: RGBA pixels, plus the same picture as ASCII art.
pub struct Processed {
    pub w: u32,
    pub h: u32,
    pub rgba: Vec<u8>,
    pub ascii: Vec<String>,
}

const BG: [u8; 3] = [14, 14, 16];
/// The dither's light ink: a warm off-white, so it reads as paper, not screen white.
const INK: [u8; 3] = [255, 236, 214];

pub fn process(bytes: &[u8], style: Style, accent: [u8; 3]) -> Option<Processed> {
    let src = image::load_from_memory(bytes).ok()?.to_rgba8();
    let square = crop_square(&src);
    let ascii = to_ascii(&square, 44, 22);
    let (w, h, rgba) = match style {
        Style::Original => {
            let img = image::imageops::resize(&square, 448, 448, FilterType::Triangle);
            (448, 448, img.into_raw())
        }
        Style::Dots => halftone(&square, 64, 7),
        Style::Dither => dither(&square, 150, 3, accent),
    };
    Some(Processed { w, h, rgba, ascii })
}

fn crop_square(img: &RgbaImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let side = w.min(h);
    image::imageops::crop_imm(img, (w - side) / 2, (h - side) / 2, side, side).to_image()
}

fn luma(p: &[u8]) -> f32 {
    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
}

/// `cells`×`cells` dots, each in a `cell`-pixel square, over a dimmed copy of the cover so
/// the picture still reads; brighter cells get bigger dots in their own (lifted) colour.
fn halftone(img: &RgbaImage, cells: u32, cell: u32) -> (u32, u32, Vec<u8>) {
    let small = image::imageops::resize(img, cells, cells, FilterType::Triangle);
    let size = cells * cell;
    let under = image::imageops::resize(img, size, size, FilterType::Triangle);
    let mut out = vec![0u8; (size * size * 4) as usize];
    // The photo underneath at about a third of its brightness.
    for (i, px) in under.pixels().enumerate() {
        for k in 0..3 {
            out[i * 4 + k] = (BG[k] as f32 * 0.4 + px.0[k] as f32 * 0.42) as u8;
        }
        out[i * 4 + 3] = 255;
    }
    let half = cell as f32 / 2.0;
    for cy in 0..cells {
        for cx in 0..cells {
            let p = small.get_pixel(cx, cy).0;
            let l = luma(&p);
            let r = half * (0.30 + 0.80 * l.sqrt()).min(1.04);
            let lift = |c: u8| ((c as f32 * 0.9 + 30.0).min(255.0)) as u8;
            let col = [lift(p[0]), lift(p[1]), lift(p[2])];
            let (ox, oy) = (cx * cell, cy * cell);
            for y in 0..cell {
                for x in 0..cell {
                    let dx = x as f32 + 0.5 - half;
                    let dy = y as f32 + 0.5 - half;
                    let cover = (r - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                    if cover <= 0.0 {
                        continue;
                    }
                    let i = (((oy + y) * size + ox + x) * 4) as usize;
                    for k in 0..3 {
                        out[i + k] = (out[i + k] as f32 + (col[k] as f32 - out[i + k] as f32) * cover) as u8;
                    }
                }
            }
        }
    }
    (size, size, out)
}

const BAYER8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

/// Ordered dither to three inks (black, accent, paper) at `res`², scaled up `scale`× with
/// hard edges so every dither pixel stays a crisp square.
fn dither(img: &RgbaImage, res: u32, scale: u32, accent: [u8; 3]) -> (u32, u32, Vec<u8>) {
    let small = image::imageops::resize(img, res, res, FilterType::Triangle);
    let size = res * scale;
    let mut out = vec![0u8; (size * size * 4) as usize];
    for y in 0..res {
        for x in 0..res {
            // A little contrast so covers don't dither to mush.
            let l = ((luma(&small.get_pixel(x, y).0) - 0.5) * 1.25 + 0.5).clamp(0.0, 1.0);
            let t = (BAYER8[(y % 8) as usize][(x % 8) as usize] as f32 + 0.5) / 64.0;
            // Two thresholds: black→accent below 0.5, accent→paper above.
            let ink = if l * 2.0 < 1.0 {
                if l * 2.0 > t { accent } else { BG }
            } else if (l - 0.5) * 2.0 > t {
                INK
            } else {
                accent
            };
            for sy in 0..scale {
                for sx in 0..scale {
                    let i = (((y * scale + sy) * size + x * scale + sx) * 4) as usize;
                    out[i..i + 4].copy_from_slice(&[ink[0], ink[1], ink[2], 255]);
                }
            }
        }
    }
    (size, size, out)
}

/// The cover as `cols`×`rows` characters (rows are half as many: glyphs are twice as tall
/// as they are wide).
fn to_ascii(img: &RgbaImage, cols: u32, rows: u32) -> Vec<String> {
    const RAMP: &[u8] = b" .:-=+*#%@";
    let small = image::imageops::resize(img, cols, rows, FilterType::Triangle);
    (0..rows)
        .map(|y| {
            (0..cols)
                .map(|x| {
                    let l = luma(&small.get_pixel(x, y).0).powf(0.9);
                    RAMP[((l * (RAMP.len() - 1) as f32).round() as usize).min(RAMP.len() - 1)] as char
                })
                .collect()
        })
        .collect()
}

/// A shared texture for cards: a dot grid whose dots grow toward the bottom-right, in white,
/// mostly transparent, laid over a gradient.
pub fn dot_texture(size: u32, cell: u32) -> Vec<u8> {
    let mut out = vec![0u8; (size * size * 4) as usize];
    let cells = size / cell;
    let half = cell as f32 / 2.0;
    for cy in 0..cells {
        for cx in 0..cells {
            let t = ((cx + cy) as f32 / (2 * cells) as f32).powf(1.6);
            let r = half * (0.08 + 0.62 * t);
            for y in 0..cell {
                for x in 0..cell {
                    let dx = x as f32 + 0.5 - half;
                    let dy = y as f32 + 0.5 - half;
                    let cover = (r - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
                    if cover > 0.0 {
                        let i = (((cy * cell + y) * size + cx * cell + x) * 4) as usize;
                        out[i..i + 4].copy_from_slice(&[255, 255, 255, (cover * 0.16 * 255.0) as u8]);
                    }
                }
            }
        }
    }
    out
}
