//! Things that float above other apps: the lyrics overlay in a screen corner, and the
//! music video popped out into a small window pinned on every workspace.

use rune::{Color, Cx, El, OverlayHandle, TextStyle, col, oklch, text};
use std::sync::OnceLock;

/// What the corner shows: the line being sung, dim lines either side, and the song.
#[derive(Default, Clone)]
pub struct LyricFloat {
    pub prev: String,
    pub line: String,
    pub next: String,
    pub song: String,
}

pub const SIZE: (f32, f32) = (520.0, 170.0);

static HANDLE: OnceLock<OverlayHandle<LyricFloat>> = OnceLock::new();

pub fn install(h: OverlayHandle<LyricFloat>) {
    let _ = HANDLE.set(h);
}

pub fn show(on: bool) {
    if let Some(h) = HANDLE.get() {
        if on { h.show() } else { h.hide() }
    }
}

pub fn set(f: LyricFloat) {
    if let Some(h) = HANDLE.get() {
        h.update(move |s, _| *s = f);
    }
}

/// Transparent, right-aligned, readable over anything: a faint dark glass behind bold text.
pub fn view(s: &LyricFloat, _: &mut Cx<LyricFloat>) -> El {
    let dim = |t: &str| text(t.to_string()).font(TextStyle::new(14.0, 600.0)).color(Color::WHITE.alpha(0.5)).truncate();
    col()
        .w_full()
        .h_full()
        .padding(16.0)
        .gap(6.0)
        .radius(18.0)
        .fill(Color::BLACK.alpha(0.38))
        .border(1.0, Color::WHITE.alpha(0.08))
        .justify_center()
        .child(text(format!("▚ {}", s.song)).font(TextStyle::new(10.5, 650.0).mono().tracking(1.0)).color(oklch(0.72, 0.17, 17.0)).truncate())
        .child(dim(&s.prev))
        .child(
            text(if s.line.is_empty() { "♪".to_string() } else { s.line.clone() })
                .key("float-line")
                .font(TextStyle::new(22.0, 780.0).tracking(-0.3))
                .color(Color::WHITE)
                .wrap(),
        )
        .child(dim(&s.next))
}

/// Pop mpv's video window out: small, floating, bottom-right, pinned to every workspace,
/// on top. Runs on a thread: it waits for the window to appear.
pub fn pop_video() {
    std::thread::spawn(|| {
        let mut addr = None;
        for _ in 0..60 {
            addr = find_window("Encore — Video");
            if addr.is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
        }
        let Some(addr) = addr else { return };
        let (mw, mh, mx, my) = focused_monitor().unwrap_or((1920, 1080, 0, 0));
        let (w, h) = (480, 270);
        let (x, y) = (mx + mw - w - 28, my + mh - h - 28);
        let win = format!("address:{addr}");
        // Pinned windows must float first.
        dispatch(&format!("hl.dsp.window.float({{ window = \"{win}\", action = \"on\" }})"));
        dispatch(&format!("hl.dsp.window.resize({{ window = \"{win}\", x = {w}, y = {h} }})"));
        dispatch(&format!("hl.dsp.window.move({{ window = \"{win}\", x = {x}, y = {y} }})"));
        if !is_pinned(&addr) {
            dispatch(&format!("hl.dsp.window.pin({{ window = \"{win}\" }})"));
        }
        dispatch(&format!("hl.dsp.window.alter_zorder({{ window = \"{win}\", mode = \"top\" }})"));
    });
}

fn hyprctl_json(args: &[&str]) -> Option<serde_json::Value> {
    let out = std::process::Command::new("hyprctl").args(args).arg("-j").output().ok()?;
    serde_json::from_slice(&out.stdout).ok()
}

fn find_window(title: &str) -> Option<String> {
    let clients = hyprctl_json(&["clients"])?;
    clients.as_array()?.iter().find(|c| c["title"].as_str() == Some(title)).and_then(|c| c["address"].as_str().map(str::to_string))
}

fn is_pinned(addr: &str) -> bool {
    hyprctl_json(&["clients"])
        .and_then(|c| c.as_array().cloned())
        .into_iter()
        .flatten()
        .any(|c| c["address"].as_str() == Some(addr) && c["pinned"].as_bool() == Some(true))
}

/// The focused monitor in layout points: (width, height, x, y), less reserved bars.
fn focused_monitor() -> Option<(i64, i64, i64, i64)> {
    let mons = hyprctl_json(&["monitors"])?;
    let m = mons.as_array()?.iter().find(|m| m["focused"].as_bool() == Some(true))?;
    let scale = m["scale"].as_f64().unwrap_or(1.0);
    let w = (m["width"].as_f64()? / scale) as i64;
    let h = (m["height"].as_f64()? / scale) as i64;
    let r = &m["reserved"];
    let (rl, rt, rr, rb) = (r[0].as_i64().unwrap_or(0), r[1].as_i64().unwrap_or(0), r[2].as_i64().unwrap_or(0), r[3].as_i64().unwrap_or(0));
    Some((w - rl - rr, h - rt - rb, m["x"].as_i64().unwrap_or(0) + rl, m["y"].as_i64().unwrap_or(0) + rt))
}

fn dispatch(lua: &str) {
    let _ = std::process::Command::new("hyprctl").arg("dispatch").arg(lua).output();
}
