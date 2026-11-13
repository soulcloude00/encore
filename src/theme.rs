//! Colours from the current Omarchy theme, followed live.
//!
//! Omarchy writes the active theme's palette to
//! `~/.local/state/omarchy/current/theme/colors.toml` and bumps `theme.generation` on every
//! switch. Glassy windows (Appearance > Windows) leave a toggle file behind; terminals then
//! draw their background at 0.65 opacity and Hyprland blurs what shows through, so Encore
//! does the same. Without Omarchy (or with a broken file) Encore keeps its own dark look.

use rune::Color;
use std::path::PathBuf;
use std::sync::RwLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// The background opacity terminals use while windows are glassy (omarchy-theme-set-glass).
const GLASS: f32 = 0.65;

#[derive(Clone, Copy)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub light: bool,
    /// Background opacity: 1.0, or `GLASS` while Omarchy's windows are glassy.
    pub opacity: f32,
}

impl Default for Theme {
    /// Encore's own look, for when there's no Omarchy theme to follow.
    fn default() -> Theme {
        Theme {
            background: rune::oklch(0.205, 0.0, 0.0),
            foreground: rune::oklch(0.97, 0.0, 0.0),
            accent: rune::oklch(0.66, 0.21, 17.0),
            light: false,
            opacity: 1.0,
        }
    }
}

impl Theme {
    /// The accent's Oklab hue in degrees and its chroma, for gradients drawn around it.
    pub fn accent_hue(&self) -> (f32, f32) {
        let [_, a, b, _] = self.accent.to_oklab();
        (b.atan2(a).to_degrees(), (a * a + b * b).sqrt())
    }

    /// Text and icons on an accent fill: white on a dark or saturated accent, the theme's
    /// background on a pale one.
    pub fn on_accent(&self) -> Color {
        if self.accent.luminance() > 0.45 {
            self.background.alpha(1.0)
        } else {
            Color::WHITE
        }
    }

    /// The accent as sRGB bytes, for inks drawn into artwork.
    pub fn accent_rgb(&self) -> [u8; 3] {
        let c = self.accent;
        [c.r, c.g, c.b].map(|v| (srgb_encode(v) * 255.0).round() as u8)
    }
}

static CURRENT: RwLock<Option<Theme>> = RwLock::new(None);
/// Bumped whenever the theme on screen changes, so `poll` knows to redraw.
static GENERATION: AtomicU64 = AtomicU64::new(0);

/// The theme on screen. The first call (the first frame) reads it from disk.
pub fn current() -> Theme {
    if let Some(t) = CURRENT.read().ok().and_then(|t| *t) {
        return t;
    }
    let t = load();
    if let Ok(mut slot) = CURRENT.write() {
        *slot = Some(t);
    }
    t
}

pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

fn state_dir() -> Option<PathBuf> {
    Some(crate::paths::state().join("omarchy"))
}

fn colors_path() -> Option<PathBuf> {
    Some(state_dir()?.join("current/theme/colors.toml"))
}

fn glassy() -> bool {
    state_dir().is_some_and(|d| d.join("toggles/hypr/window-glassy.lua").exists())
}

/// The theme as Omarchy has it now; Encore's own colours for anything it doesn't say.
fn load() -> Theme {
    let mut t = Theme::default();
    if let Some(text) = colors_path().and_then(|p| std::fs::read_to_string(p).ok()) {
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            match key.trim() {
                "background" => t.background = hex(value).unwrap_or(t.background),
                "foreground" => t.foreground = hex(value).unwrap_or(t.foreground),
                "accent" => t.accent = hex(value).unwrap_or(t.accent),
                "mode" => t.light = value == "light",
                _ => {}
            }
        }
    }
    t.opacity = if glassy() { GLASS } else { 1.0 };
    t
}

/// What a change shows up in: the theme's generation stamp (or its colours, when there's
/// no stamp) and whether windows are glassy.
fn fingerprint() -> (Option<SystemTime>, Option<SystemTime>, bool) {
    let mtime = |p: Option<PathBuf>| p.and_then(|p| std::fs::metadata(p).ok()?.modified().ok());
    (
        mtime(state_dir().map(|d| d.join("current/theme.generation"))),
        mtime(colors_path()),
        glassy(),
    )
}

/// Load the theme now, then follow it: a thread checks two timestamps every 2 seconds (no
/// file reads unless one changed) and wakes the app when the theme or glassiness changes.
pub fn follow(waker: rune::Waker) {
    *CURRENT.write().unwrap() = Some(load());
    std::thread::Builder::new()
        .name("theme-watch".into())
        .spawn(move || {
            let mut seen = fingerprint();
            loop {
                std::thread::sleep(Duration::from_secs(2));
                let now = fingerprint();
                if now == seen {
                    continue;
                }
                seen = now;
                // theme-set rewrites several files: let it finish before reading colors.toml.
                std::thread::sleep(Duration::from_millis(300));
                *CURRENT.write().unwrap() = Some(load());
                GENERATION.fetch_add(1, Ordering::Relaxed);
                waker();
            }
        })
        .ok();
}

fn srgb_decode(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn srgb_encode(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// `#rrggbb` (sRGB) to Rune's linear colour.
fn hex(s: &str) -> Option<Color> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let byte = |i: usize| {
        u8::from_str_radix(&h[i..i + 2], 16)
            .ok()
            .map(|v| srgb_decode(v as f32 / 255.0))
    };
    Some(Color {
        r: byte(0)?,
        g: byte(2)?,
        b: byte(4)?,
        a: 1.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips_through_linear() {
        let c = hex("#8071b3").unwrap();
        let t = Theme {
            accent: c,
            ..Theme::default()
        };
        assert_eq!(t.accent_rgb(), [0x80, 0x71, 0xb3]);
        assert!(hex("8071b3").is_none() && hex("#81").is_none());
    }
}
