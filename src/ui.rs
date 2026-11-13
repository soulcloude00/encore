//! Encore's look: sidebar, a toolbar with a centred "now playing" display, big bold page
//! titles, a top-result card, artwork shelves and clean track lists. Dark, with a red accent.

use crate::ytm::Track;
use crate::art::Style;
use crate::lyrics::Lyrics;
use crate::{App, MOODS, Page, Quality, Repeat, Vocal, local_hour};
use rune::{
    Color, Cx, Drag, El, Icon, Phase, ScrollSpec, Spring, SurfaceFit, TextStyle, Tx, col, el, icon, oklch,
    row, spacer, text, text_input,
};

// ------------------------------------------------------------------ palette & type

pub struct Pal {
    pub bg: Color,
    pub side: Color,
    pub bar: Color,
    pub card: Color,
    pub well: Color,
    pub hover: Color,
    pub line: Color,
    pub text: Color,
    pub dim: Color,
    pub faint: Color,
    pub accent: Color,
    pub accent_soft: Color,
    /// Text and icons drawn on an accent fill.
    pub on_accent: Color,
    /// The accent's hue (degrees) and chroma, for the gradients drawn around it.
    pub hue: f32,
    pub chroma: f32,
}

/// The palette, from the current Omarchy theme (see `theme.rs`). Only the window background
/// carries the theme's opacity; every surface above it is a see-through tint of the
/// foreground, so panels stay as glassy as the window and work on light themes too.
pub fn pal() -> Pal {
    let t = crate::theme::current();
    let (bg, fg) = (t.background, t.foreground);
    let (hue, chroma) = t.accent_hue();
    Pal {
        bg: bg.alpha(t.opacity),
        side: fg.alpha(0.045),
        bar: fg.alpha(0.025),
        card: fg.alpha(0.07),
        well: fg.alpha(0.11),
        hover: fg.alpha(0.06),
        line: fg.alpha(0.09),
        text: fg,
        dim: fg.mix(bg, 0.3),
        faint: fg.mix(bg, 0.5),
        accent: t.accent,
        accent_soft: t.accent.alpha(0.18),
        on_accent: t.on_accent(),
        hue,
        chroma,
    }
}

const LARGE_TITLE: TextStyle = TextStyle::new(30.0, 760.0).tracking(-0.6);
const SECTION: TextStyle = TextStyle::new(19.0, 700.0).tracking(-0.3);
const HERO: TextStyle = TextStyle::new(24.0, 720.0).tracking(-0.4);
const TITLE: TextStyle = TextStyle::new(13.5, 600.0);
const BODY: TextStyle = TextStyle::new(13.0, 400.0);
const SMALL: TextStyle = TextStyle::new(11.5, 500.0);
const SIDE_HEAD: TextStyle = TextStyle::new(11.0, 650.0);
const NAV: TextStyle = TextStyle::new(13.5, 500.0);
const NUM: TextStyle = TextStyle::new(11.5, 500.0).mono();
/// Terminal-flavoured labels for the dither look.
const MONO_LABEL: TextStyle = TextStyle::new(10.5, 650.0).mono().tracking(1.4);
const ASCII: TextStyle = TextStyle::new(8.2, 600.0).mono().lh(8.6);

const SIDEBAR_W: f32 = 236.0;
const LCD_W: f32 = 500.0;
const CARD: f32 = 172.0;

// ------------------------------------------------------------------ icons (24×24 SVG bodies)

macro_rules! ico {
    ($name:ident, $body:expr) => {
        static $name: Icon = Icon { name: stringify!($name), view: 24.0, body: $body };
    };
}
ico!(PLAY, r#"<path d="M7.5 4.8v14.4a1 1 0 0 0 1.52.85l11.3-7.2a1 1 0 0 0 0-1.7L9.02 3.95A1 1 0 0 0 7.5 4.8z" fill="currentColor"/>"#);
ico!(PAUSE, r#"<rect x="5.5" y="4" width="4.6" height="16" rx="1.3" fill="currentColor"/><rect x="13.9" y="4" width="4.6" height="16" rx="1.3" fill="currentColor"/>"#);
ico!(NEXT, r#"<path d="M2.5 6.1v11.8a.9.9 0 0 0 1.38.76L12 13.5v4.4a.9.9 0 0 0 1.38.76l8.6-5.9a.9.9 0 0 0 0-1.5l-8.6-5.9A.9.9 0 0 0 12 6.1v4.4L3.88 5.34A.9.9 0 0 0 2.5 6.1z" fill="currentColor"/>"#);
ico!(PREV, r#"<path d="M21.5 6.1v11.8a.9.9 0 0 1-1.38.76L12 13.5v4.4a.9.9 0 0 1-1.38.76l-8.6-5.9a.9.9 0 0 1 0-1.5l8.6-5.9A.9.9 0 0 1 12 6.1v4.4l8.12-5.16a.9.9 0 0 1 1.38.76z" fill="currentColor"/>"#);
ico!(SHUFFLE, r#"<path d="M3 7h3.5c2 0 3.2 1 4.3 2.6M3 17h3.5c2 0 3.2-1 4.4-2.8l2.3-3.6C14.3 8.9 15.6 7 18 7h3M17.5 3.8 21 7l-3.5 3.2M14.3 14.6c1 1.4 2 2.4 3.7 2.4h3M17.5 13.8 21 17l-3.5 3.2" stroke="currentColor" stroke-width="1.9" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#);
ico!(REPEAT, r#"<path d="M4 11V9.5A2.5 2.5 0 0 1 6.5 7H20M16.5 3.5 20 7l-3.5 3.5M20 13v1.5a2.5 2.5 0 0 1-2.5 2.5H4M7.5 20.5 4 17l3.5-3.5" stroke="currentColor" stroke-width="1.9" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#);
ico!(SEARCH, r#"<circle cx="10.5" cy="10.5" r="6.5" stroke="currentColor" stroke-width="2.1" fill="none"/><path d="M15.5 15.5 20.5 20.5" stroke="currentColor" stroke-width="2.1" stroke-linecap="round"/>"#);
ico!(NOTE, r#"<path d="M9 18V6.2l11-2.2v12" stroke="currentColor" stroke-width="1.9" fill="none" stroke-linejoin="round"/><circle cx="6.5" cy="18" r="2.6" fill="currentColor"/><circle cx="17.5" cy="16" r="2.6" fill="currentColor"/>"#);
ico!(LIST, r#"<path d="M9 6.5h11M9 12h11M9 17.5h11" stroke="currentColor" stroke-width="2" stroke-linecap="round"/><circle cx="4.5" cy="6.5" r="1.4" fill="currentColor"/><circle cx="4.5" cy="12" r="1.4" fill="currentColor"/><circle cx="4.5" cy="17.5" r="1.4" fill="currentColor"/>"#);
ico!(CLOCK, r#"<circle cx="12" cy="12" r="8.5" stroke="currentColor" stroke-width="2" fill="none"/><path d="M12 7.5V12l3 2" stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#);
ico!(SPEAKER, r#"<path d="M4 9.5h3.2L11.5 6v12l-4.3-3.5H4z" fill="currentColor"/><path d="M15 9.2a4 4 0 0 1 0 5.6M17.6 6.8a7.4 7.4 0 0 1 0 10.4" stroke="currentColor" stroke-width="1.8" fill="none" stroke-linecap="round"/>"#);
ico!(SPEAKER_LOW, r#"<path d="M4 9.5h3.2L11.5 6v12l-4.3-3.5H4z" fill="currentColor"/>"#);
ico!(HOUSE, r#"<path d="M4 10.5 12 4l8 6.5V19a1 1 0 0 1-1 1h-4.5v-5.5h-5V20H5a1 1 0 0 1-1-1z" stroke="currentColor" stroke-width="1.9" fill="none" stroke-linejoin="round"/>"#);
ico!(RADIO, r#"<circle cx="12" cy="12" r="2.2" fill="currentColor"/><path d="M8.2 8.2a5.4 5.4 0 0 0 0 7.6M15.8 8.2a5.4 5.4 0 0 1 0 7.6M5.4 5.4a9.3 9.3 0 0 0 0 13.2M18.6 5.4a9.3 9.3 0 0 1 0 13.2" stroke="currentColor" stroke-width="1.9" fill="none" stroke-linecap="round"/>"#);
ico!(HEART, r#"<path d="M12 20.3s-7.8-4.6-7.8-10.3A4.3 4.3 0 0 1 12 7.6a4.3 4.3 0 0 1 7.8 2.4c0 5.7-7.8 10.3-7.8 10.3z" stroke="currentColor" stroke-width="1.9" fill="none" stroke-linejoin="round"/>"#);
ico!(HEART_FILL, r#"<path d="M12 20.3s-7.8-4.6-7.8-10.3A4.3 4.3 0 0 1 12 7.6a4.3 4.3 0 0 1 7.8 2.4c0 5.7-7.8 10.3-7.8 10.3z" fill="currentColor"/>"#);
ico!(QUOTE, r#"<path d="M5 7.5h5.5v5.2c0 3-1.6 4.8-4.5 5.3M13.5 7.5H19v5.2c0 3-1.6 4.8-4.5 5.3" stroke="currentColor" stroke-width="1.9" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#);
ico!(FILM, r#"<rect x="3.5" y="5.5" width="17" height="13" rx="2.5" stroke="currentColor" stroke-width="1.9" fill="none"/><path d="M10.2 9.4v5.2l4.4-2.6z" fill="currentColor"/>"#);
ico!(MIC, r#"<rect x="9" y="3.5" width="6" height="11" rx="3" stroke="currentColor" stroke-width="1.9" fill="none"/><path d="M5.8 11.5a6.2 6.2 0 0 0 12.4 0M12 17.7v3" stroke="currentColor" stroke-width="1.9" fill="none" stroke-linecap="round"/>"#);
ico!(GEAR, r#"<circle cx="12" cy="12" r="3.2" stroke="currentColor" stroke-width="2" fill="none"/><path d="M12 2.8v2.4M12 18.8v2.4M2.8 12h2.4M18.8 12h2.4M5.5 5.5l1.7 1.7M16.8 16.8l1.7 1.7M5.5 18.5l1.7-1.7M16.8 7.2l1.7-1.7" stroke="currentColor" stroke-width="2" stroke-linecap="round"/>"#);
ico!(CHECK, r#"<path d="M5 12.5l4.5 4.5L19 7.5" stroke="currentColor" stroke-width="2.4" fill="none" stroke-linecap="round" stroke-linejoin="round"/>"#);
ico!(BARS, r#"<rect x="4" y="9" width="3.2" height="11" rx="1" fill="currentColor"/><rect x="10.4" y="4" width="3.2" height="16" rx="1" fill="currentColor"/><rect x="16.8" y="12" width="3.2" height="8" rx="1" fill="currentColor"/>"#);

// ------------------------------------------------------------------ root

/// How much room the window gives: Hyprland tiles windows, so Encore is often half a screen.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Fit {
    Narrow,
    Medium,
    Wide,
}

fn fit_for(r: [f32; 4]) -> Fit {
    match r[2] {
        w if w < 820.0 => Fit::Narrow,
        w if w < 1180.0 => Fit::Medium,
        _ => Fit::Wide,
    }
}

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let p = pal();
    let fit = cx.layout_with("root", fit_for).unwrap_or(Fit::Wide);
    row().key("root").w_full().h_full().fill(p.bg).child(sidebar(s, &p, fit)).child(
        col().grow(1.0).shrink().min_w(0.0).h_full().child(toolbar(s, &p, fit)).child(page(s, &p, fit)),
    )
}

fn search_field(s: &App, p: &Pal, h: f32) -> El {
    let focused = s.query.focused;
    row()
        .key("search-box")
        .w_full()
        .h(h)
        .px(10.0)
        .gap(7.0)
        .radius(8.0)
        .fill(p.well)
        .border(if focused { 2.0 } else { 0.0 }, p.accent.alpha(0.7))
        .center_y()
        .child(icon(&SEARCH, 14.0).color(p.faint))
        .child(
            text_input(&s.query, BODY)
                .key("search")
                .single_line()
                .grow(1.0)
                .color(p.text)
                .placeholder("Search", p.faint)
                .on_edit(|s: &mut App| &mut s.query)
                .on_submit(|s: &mut App, tx: &mut Tx| {
                    s.run_search();
                    tx.blur();
                }),
        )
}

fn app_icon(size: f32) -> El {
    let p = pal();
    el().size(size, size)
        .radius(size * 0.27)
        .gradient(p.accent, p.accent.mix(Color::BLACK, 0.3))
        .center()
        .child(icon(&NOTE, size * 0.58).color(p.on_accent))
}

// ------------------------------------------------------------------ sidebar

fn sidebar(s: &App, p: &Pal, fit: Fit) -> El {
    if fit != Fit::Wide {
        return rail(s, p);
    }
    let brand = row()
        .gap(8.0)
        .center_y()
        .pl(4.0)
        .child(app_icon(26.0))
        .child(text("Encore").font(TextStyle::new(16.0, 720.0).tracking(-0.3)).color(p.text));

    col()
        .key("sidebar")
        .w(SIDEBAR_W)
        .h_full()
        .px(14.0)
        .pt(20.0)
        .pb(14.0)
        .gap(4.0)
        .fill(p.side)
        .border_sides([0.0, 1.0, 0.0, 0.0], p.line)
        .child(brand)
        .child(el().h(14.0))
        .child(search_field(s, p, 32.0))
        .child(el().h(18.0))
        .child(nav(s, p, Page::Home, &HOUSE, "Home"))
        .child(nav(s, p, Page::Search, &SEARCH, "Search"))
        .child(el().h(14.0))
        .child(text("Library").font(SIDE_HEAD).color(p.faint).pl(8.0).pb(4.0))
        .child(nav(s, p, Page::Queue, &LIST, "Up Next"))
        .child(nav(s, p, Page::Recent, &CLOCK, "Recently Played"))
        .child(nav(s, p, Page::Liked, &HEART, "Liked Songs"))
        .child(spacer())
        .child(nav(s, p, Page::Settings, &GEAR, "Settings"))
}

/// The sidebar folded to icons, for half-screen tiles.
fn rail(s: &App, p: &Pal) -> El {
    let item = |page: Page, glyph: &'static Icon, key: &'static str| {
        let on = s.page == page;
        el().key(key)
            .size(40.0, 40.0)
            .radius(10.0)
            .center()
            .fill(if on { p.hover.alpha(0.12) } else { Color::TRANSPARENT })
            .hover(if on { p.hover.alpha(0.12) } else { p.hover })
            .on_press(move |s: &mut App| {
                s.page = page;
                if page == Page::Search {
                    s.query.focused = true;
                }
            })
            .child(icon(glyph, 19.0).color(if on { p.accent } else { p.dim }))
    };
    col()
        .key("rail")
        .w(60.0)
        .h_full()
        .pt(16.0)
        .pb(12.0)
        .gap(6.0)
        .items_center()
        .fill(p.side)
        .border_sides([0.0, 1.0, 0.0, 0.0], p.line)
        .child(app_icon(30.0))
        .child(el().h(12.0))
        .child(item(Page::Home, &HOUSE, "rail-home"))
        .child(item(Page::Search, &SEARCH, "rail-search"))
        .child(item(Page::Queue, &LIST, "rail-queue"))
        .child(item(Page::Recent, &CLOCK, "rail-recent"))
        .child(item(Page::Liked, &HEART, "rail-liked"))
        .child(spacer())
        .child(item(Page::Settings, &GEAR, "rail-settings"))
}

fn nav(s: &App, p: &Pal, page: Page, glyph: &'static Icon, label: &'static str) -> El {
    let on = s.page == page;
    row()
        .key(format!("nav-{label}"))
        .w_full()
        .h(32.0)
        .px(8.0)
        .gap(10.0)
        .radius(7.0)
        .center_y()
        .fill(if on { p.hover.alpha(0.12) } else { Color::TRANSPARENT })
        .hover(if on { p.hover.alpha(0.12) } else { p.hover })
        .on_press(move |s: &mut App| s.page = page)
        .child(icon(glyph, 17.0).color(p.accent))
        .child(text(label).font(NAV).color(p.text))
}

// ------------------------------------------------------------------ toolbar

fn toolbar(s: &App, p: &Pal, fit: Fit) -> El {
    let paused = s.current.is_none() || s.status.paused;
    let repeat_glyph = row()
        .child(icon(&REPEAT, 17.0).color(if s.repeat == Repeat::Off { p.dim } else { p.accent }))
        .child(if s.repeat == Repeat::One { text("1").font(TextStyle::new(9.0, 800.0)).color(p.accent) } else { el() });
    let narrow = fit == Fit::Narrow;
    let mut controls = row().gap(if narrow { 2.0 } else { 4.0 }).center_y();
    if !narrow {
        controls = controls.child(tool_btn("shuffle", icon(&SHUFFLE, 17.0).color(if s.shuffle { p.accent } else { p.dim }), 34.0, p, |s| s.toggle_shuffle()));
    }
    controls = controls
        .child(tool_btn("prev", icon(&PREV, 21.0).color(p.text), 36.0, p, |s| s.prev()))
        .child(tool_btn("play", icon(if paused { &PLAY } else { &PAUSE }, 25.0).color(p.text), 42.0, p, |s| s.toggle()))
        .child(tool_btn("next", icon(&NEXT, 21.0).color(p.text), 36.0, p, |s| s.next()));
    if !narrow {
        controls = controls.child(tool_btn("repeat", repeat_glyph, 34.0, p, |s| s.cycle_repeat()));
    }

    // Lyrics, video and vocal focus: lit in the accent while on.
    let vocal_on = s.vocal != Vocal::Normal;
    let mut extras = row()
        .gap(2.0)
        .center_y()
        .child(tool_btn("lyrics-btn", icon(&QUOTE, 17.0).color(if s.page == Page::Lyrics { p.accent } else { p.dim }), 32.0, p, |s| s.toggle_lyrics()))
        .child(tool_btn("video-btn", icon(&FILM, 18.0).color(if s.video { p.accent } else { p.dim }), 32.0, p, |s| s.toggle_video()));
    extras = extras.child(
        row()
            .key("vocal-btn")
            .h(32.0)
            .px(if vocal_on && !narrow { 10.0 } else { 7.0 })
            .gap(5.0)
            .radius(16.0)
            .center_y()
            .fill(if vocal_on { p.accent_soft } else { Color::TRANSPARENT })
            .hover(p.hover)
            .press_scale(0.92)
            .on_press(|s: &mut App| s.cycle_vocal())
            .child(icon(&MIC, 17.0).color(if vocal_on { p.accent } else { p.dim }))
            .child(if vocal_on {
                // Separating: the progress; on real stems: "AI"; else the mode's name.
                let label = match (s.stem_progress(), s.stem_playing.is_some()) {
                    (Some(pr), _) => format!("{} {:.0}%", if narrow { "" } else { s.vocal.label() }, pr * 100.0),
                    (None, true) => format!("{} · AI", if narrow { "" } else { s.vocal.label() }),
                    _ if narrow => String::new(),
                    _ => s.vocal.label().to_string(),
                };
                text(label.trim().to_string()).font(TextStyle::new(11.0, 650.0)).color(p.accent)
            } else {
                el()
            }),
    );

    let bar = row()
        .key("toolbar")
        .w_full()
        .h(72.0)
        .px(if narrow { 12.0 } else { 20.0 })
        .gap(if narrow { 10.0 } else { 18.0 })
        .center_y()
        .fill(p.bar)
        .border_sides([0.0, 0.0, 1.0, 0.0], p.line);
    match fit {
        // Wide: controls | centred display | volume, with equal side columns so the display
        // sits in the middle of the toolbar.
        Fit::Wide => bar
            .child(row().w(0.0).grow(1.0).center_y().child(controls))
            .child(lcd(s, p, fit).w(LCD_W))
            .child(row().w(0.0).grow(1.0).gap(10.0).center_y().child(spacer()).child(extras).child(volume(s, p, 80.0))),
        // Medium: the display takes the room between the controls and a shorter volume.
        Fit::Medium => bar.child(controls).child(lcd(s, p, fit).grow(1.0)).child(extras).child(volume(s, p, 56.0)),
        // Narrow: the essentials; ↑ ↓ still change the volume.
        Fit::Narrow => bar.child(controls).child(lcd(s, p, fit).grow(1.0)).child(extras),
    }
}

fn tool_btn(key: &str, glyph: El, size: f32, p: &Pal, f: impl Fn(&mut App) + 'static) -> El {
    el().key(key).size(size, size).radius(size / 2.0).center().hover(p.hover).press_scale(0.9).on_press(f).child(glyph)
}

/// The centred now-playing display: artwork, title, artist, and a progress line you can scrub.
fn lcd(s: &App, p: &Pal, fit: Fit) -> El {
    let st = &s.status;
    let track = s.now_playing();
    let frac = s
        .scrub
        .unwrap_or(if st.duration > 0.0 { (st.time / st.duration).clamp(0.0, 1.0) as f32 } else { 0.0 });
    let shown_time = if s.scrub.is_some() { st.duration * frac as f64 } else { st.time };

    let art = match track {
        Some(t) => cover(s, t, 56.0, 0.0, p),
        None => el(),
    };

    let (title, sub) = match track {
        Some(t) => {
            let sub = match (&st.error, st.loading) {
                (Some(e), _) => e.clone(),
                (None, true) => "Loading…".into(),
                _ if t.album.is_empty() => t.artist.clone(),
                _ => format!("{} — {}", t.artist, t.album),
            };
            (t.title.clone(), sub)
        }
        None => (String::new(), String::new()),
    };

    let info: El = if track.is_none() {
        col().grow(1.0).h_full().center().child(icon(&NOTE, 20.0).color(p.faint.alpha(0.6)))
    } else {
        col()
            .grow(1.0)
            .shrink()
            .min_w(0.0)
            .h_full()
            .px(12.0)
            .pt(7.0)
            .gap(1.0)
            .items_center()
            .child(
                row()
                    .w_full()
                    .gap(8.0)
                    .center_y()
                    .child(row().w(0.0).grow(1.0).gap(2.0).child(lcd_actions(s, p)))
                    .child(text(title).font(TITLE).color(p.text).truncate().shrink().min_w(0.0))
                    .child(el().w(0.0).grow(1.0).child(spacer()).child(quality_badge(s, p, fit))),
            )
            .child(text(sub).font(SMALL).color(if st.error.is_some() { p.accent } else { p.dim }).truncate())
            .child(spacer())
            .child(
                row()
                    .w_full()
                    .gap(8.0)
                    .center_y()
                    .h(16.0)
                    .child(text(crate::ui::clock(shown_time)).font(NUM).color(p.faint).w(34.0))
                    .child(scrubber(frac, p))
                    .child(text(format!("-{}", clock((st.duration - shown_time).max(0.0)))).font(NUM).color(p.faint).w(38.0)),
            )
    };

    row()
        .key("lcd")
        .shrink()
        .min_w(0.0)
        .h(58.0)
        .radius(9.0)
        .fill(p.card)
        .border(1.0, p.line)
        .center_y()
        .clip_rounded(9.0)
        .child(el().pl(1.0).child(art))
        .child(info)
}

/// ♥ and song radio for what's playing, inside the now-playing display.
fn lcd_actions(s: &App, p: &Pal) -> El {
    let Some(t) = s.now_playing().cloned() else { return el() };
    let liked = s.taste.is_liked(&t.id);
    let on_air = s.station.as_ref().is_some_and(|st| st.seed.id == t.id);
    let (a, b) = (t.clone(), t);
    row()
        .gap(0.0)
        .child(
            el().key("lcd-like")
                .size(22.0, 22.0)
                .radius(11.0)
                .center()
                .hover(p.hover)
                .on_press(move |s: &mut App| s.toggle_like(&a))
                .child(icon(if liked { &HEART_FILL } else { &HEART }, 13.0).color(if liked { p.accent } else { p.faint })),
        )
        .child(
            el().key("lcd-radio")
                .size(22.0, 22.0)
                .radius(11.0)
                .center()
                .hover(p.hover)
                .on_press(move |s: &mut App| if !on_air { s.start_radio(b.clone()) })
                .child(icon(&RADIO, 13.0).color(if s.station.is_some() { p.accent } else { p.faint })),
        )
}

/// What's streaming right now ("OPUS 141"), dimmed until it's known. Click it for Settings.
fn quality_badge(s: &App, p: &Pal, fit: Fit) -> El {
    let st = &s.status;
    if s.current.is_none() || st.codec.is_empty() {
        return el();
    }
    let codec = match st.codec.as_str() {
        "aac" => "AAC".to_string(),
        "opus" => "OPUS".to_string(),
        c => c.to_uppercase(),
    };
    let label = if fit == Fit::Narrow || st.bitrate <= 0.0 {
        codec
    } else {
        format!("{codec} {}", (st.bitrate / 1000.0).round() as u32)
    };
    let best = s.quality == Quality::Best;
    el().key("quality-badge")
        .h(16.0)
        .px(5.0)
        .radius(4.0)
        .border(1.0, if best { p.accent.alpha(0.6) } else { p.faint.alpha(0.6) })
        .center()
        .hover(p.hover)
        .on_press(|s: &mut App| s.page = Page::Settings)
        .child(text(label).font(TextStyle::new(9.0, 700.0).tracking(0.5)).color(if best { p.accent } else { p.dim }))
}

fn scrubber(frac: f32, p: &Pal) -> El {
    // The hit area is taller than the line, so it's easy to grab.
    el().key("scrub")
        .grow(1.0)
        .h(12.0)
        .center_y()
        .on_drag(|s: &mut App, d: &mut Drag, _tx: &mut Tx| {
            let f = (d.x / d.w.max(1.0)).clamp(0.0, 1.0);
            match d.phase {
                Phase::Down | Phase::Move => s.scrub = Some(f),
                Phase::Up => {
                    s.scrub = None;
                    if !d.cancelled() {
                        s.seek_to(f);
                    }
                }
            }
        })
        .child(
            el().w_full().h(3.0).radius(1.5).fill(p.line).child(
                el().key("scrub-fill").w_frac(frac).h(3.0).radius(1.5).fill(p.dim).motion(Spring::new(0.2, 0.0)),
            ),
        )
}

fn volume(s: &App, p: &Pal, width: f32) -> El {
    let frac = s.volume / 100.0;
    row()
        .gap(8.0)
        .center_y()
        .child(icon(&SPEAKER_LOW, 15.0).color(p.dim))
        .child(
            el().key("volume")
                .w(width)
                .h(14.0)
                .center_y()
                .on_drag(|s: &mut App, d: &mut Drag, _tx: &mut Tx| {
                    s.set_volume((d.x / d.w.max(1.0)).clamp(0.0, 1.0) * 100.0);
                })
                .child(
                    el().w_full().h(4.0).radius(2.0).fill(p.line).child(
                        el().w_frac(frac).h(4.0).radius(2.0).fill(p.dim),
                    ),
                ),
        )
        .child(icon(&SPEAKER, 17.0).color(p.dim))
}

// ------------------------------------------------------------------ pages

fn page(s: &App, p: &Pal, fit: Fit) -> El {
    let mut content = match s.page {
        Page::Home => home_page(s, p, fit),
        Page::Search => search_page(s, p, fit),
        Page::Queue => queue_page(s, p, fit),
        Page::Recent => recent_page(s, p, fit),
        Page::Liked => liked_page(s, p, fit),
        Page::Lyrics => lyrics_page(s, p, fit),
        Page::Settings => settings_page(s, p),
    };
    // With the sidebar folded to icons, search lives at the top of the Search page.
    if fit != Fit::Wide && s.page == Page::Search {
        content = col().w_full().gap(22.0).child(row().w_full().max_w(520.0).child(search_field(s, p, 36.0))).child(content);
    }
    col()
        .key("page")
        .w_full()
        .grow(1.0)
        .shrink()
        .min_h(0.0)
        .child(
            col()
                .key(match s.page {
                    Page::Home => "content-home",
                    Page::Search => "content-search",
                    Page::Queue => "content-queue",
                    Page::Recent => "content-recent",
                    Page::Liked => "content-liked",
                    Page::Lyrics => "content-lyrics",
                    Page::Settings => "content-settings",
                })
                .w_full()
                .px(if fit == Fit::Narrow { 20.0 } else { 36.0 })
                .pt(if fit == Fit::Narrow { 20.0 } else { 28.0 })
                .pb(40.0)
                .gap(30.0)
                .child(content)
                .scroll(ScrollSpec { stick_bottom: false })
                .scroll_edge_fade(16.0)
                .scroll_margin(if s.page == Page::Lyrics { 220.0 } else { 0.0 })
                .grow(1.0)
                .shrink()
                .min_h(0.0),
        )
}

fn large_title(t: impl Into<String>, p: &Pal) -> El {
    text(t.into()).font(LARGE_TITLE).color(p.text)
}

fn search_page(s: &App, p: &Pal, fit: Fit) -> El {
    let mut c = col().w_full().gap(30.0);
    if s.results.is_empty() {
        c = c.child(large_title("Search", p));
        let (head, sub) = if s.search.is_pending() {
            ("Searching…".to_string(), format!("Looking for “{}”", s.query.text.trim()))
        } else if let Some(m) = &s.message {
            if m == "No songs found" {
                ("No results".into(), "Check the spelling, or try something else.".into())
            } else {
                ("Something went wrong".into(), m.clone())
            }
        } else {
            ("Find something to play".into(), "Search YouTube Music for songs, artists and albums.".into())
        };
        c = c.child(
            col()
                .w_full()
                .pt(70.0)
                .pb(30.0)
                .gap(8.0)
                .items_center()
                .child(el().size(76.0, 76.0).radius(38.0).fill(p.accent_soft).center().child(icon(&SEARCH, 32.0).color(p.accent)))
                .child(el().h(6.0))
                .child(text(head).font(SECTION).color(p.text))
                .child(text(sub).font(BODY).color(p.dim))
                .child(
                    row()
                        .w_full()
                        .flex_wrap()
                        .justify_center()
                        .pt(14.0)
                        .gap(16.0)
                        .child(hint("/", "search", p))
                        .child(hint("Space", "play / pause", p))
                        .child(hint("← →", "seek", p))
                        .child(hint("↑ ↓", "volume", p)),
                ),
        );
        if !s.history.is_empty() {
            c = c.child(shelf(s, p, "Recently Played", &s.history, "recent"));
        }
        return c;
    }

    let title = if s.search.is_pending() { "Searching…".to_string() } else { format!("Results for “{}”", s.searched) };
    c = c.child(large_title(title, p));

    let top = &s.results[0];
    let top_card = {
        let t = top.clone();
        col()
            .key("top-result")
            .w_full()
            .h(262.0)
            .padding(20.0)
            .gap(14.0)
            .radius(12.0)
            .items_start()
            .fill(p.card)
            .hover(p.well)
            .on_press(move |s: &mut App| s.play_from(s.results.clone(), 0))
            .child(cover(s, &t, 116.0, 8.0, p).shadow(8.0, 20.0, Color::BLACK.alpha(0.4)))
            .child(
                col()
                    .w_full()
                    .gap(4.0)
                    .child(text(t.title.clone()).font(HERO).color(p.text).truncate())
                    .child(text(format!("Song · {}", t.artist)).font(BODY).color(p.dim).truncate()),
            )
            .child(spacer())
            .child(
                row()
                    .h(30.0)
                    .px(14.0)
                    .gap(6.0)
                    .radius(15.0)
                    .fill(p.accent)
                    .center_y()
                    .child(icon(&PLAY, 13.0).color(p.on_accent))
                    .child(text("Play").font(TITLE).color(p.on_accent)),
            )
    };

    let mut songs = col().grow(1.0).shrink().min_w(0.0).gap(0.0);
    for (i, t) in s.results.iter().enumerate().skip(1).take(4) {
        songs = songs.child(compact_row(s, p, t, i));
    }
    // Side by side when there's room; stacked in a half-screen tile. Each heading shares a
    // column with what it heads, so the two can't drift apart.
    let top_section = if fit == Fit::Narrow {
        col()
            .w_full()
            .gap(12.0)
            .child(text("Top Result").font(SECTION).color(p.text))
            .child(row().w_full().child(top_card))
            .child(el().h(14.0))
            .child(text("Songs").font(SECTION).color(p.text))
            .child(songs.w_full())
    } else {
        let card_w = if fit == Fit::Wide { 360.0 } else { 300.0 };
        row()
            .w_full()
            .gap(28.0)
            .child(col().w(card_w).gap(12.0).child(text("Top Result").font(SECTION).color(p.text)).child(top_card.w_full()))
            .child(col().w(0.0).grow(1.0).gap(12.0).child(text("Songs").font(SECTION).color(p.text)).child(songs.w_full()))
    };
    c = c.child(top_section);

    if s.results.len() > 5 {
        c = c.child(shelf(s, p, "More Songs", &s.results[5..], "more"));
    }
    c.child(track_list(s, p, fit, "All Songs", &s.results, |s: &mut App, i| s.play_from(s.results.clone(), i)))
}

fn hint(k: &str, what: &str, p: &Pal) -> El {
    row()
        .gap(6.0)
        .center_y()
        .child(el().h(22.0).px(7.0).radius(6.0).border(1.0, p.line).center().child(text(k.to_string()).font(NUM).color(p.dim)))
        .child(text(what.to_string()).font(SMALL).color(p.faint))
}

fn queue_page(s: &App, p: &Pal, fit: Fit) -> El {
    let mut c = col().w_full().gap(30.0).child(large_title("Up Next", p));
    if let Some(st) = &s.station {
        let mut note = format!("Based on “{}” by {}. Plays on forever; skip an artist twice and they're gone.", st.seed.title, st.seed.artist);
        if !st.banned.is_empty() {
            note = format!("{note} Skipped: {}.", st.banned.iter().cloned().collect::<Vec<_>>().join(", "));
        }
        c = c.child(
            row()
                .key("station-card")
                .w_full()
                .padding(16.0)
                .gap(16.0)
                .radius(14.0)
                .gradient(p.accent.alpha(0.3), p.accent.alpha(0.05))
                .border(1.0, p.line)
                .center_y()
                .child(cover(s, &st.seed, 64.0, 8.0, p))
                .child(
                    col()
                        .w(0.0)
                        .grow(1.0)
                        .gap(4.0)
                        .child(text("▚ SONG RADIO").font(MONO_LABEL).color(p.accent))
                        .child(text(format!("{} Radio", st.seed.title)).font(TextStyle::new(18.0, 720.0)).color(p.text).truncate())
                        .child(text(note).font(SMALL).color(p.dim).wrap()),
                )
                .child(
                    row()
                        .key("stop-radio")
                        .h(30.0)
                        .px(14.0)
                        .radius(15.0)
                        .border(1.0, p.text.alpha(0.25))
                        .hover(p.hover)
                        .center_y()
                        .on_press(|s: &mut App| s.stop_radio())
                        .child(text("Stop Radio").font(TITLE).color(p.text)),
                ),
        );
    }
    match s.current {
        None => c.child(text("Nothing queued. Play a song and the rest of its list lines up here.").font(BODY).color(p.dim)),
        Some(cur) => {
            c = c.child(track_list(s, p, fit, "Now Playing", &s.queue[cur..=cur], move |s: &mut App, _| s.play_index(cur)));
            let rest = &s.queue[cur + 1..];
            if rest.is_empty() {
                c.child(text("That's the last song in the queue.").font(BODY).color(p.dim))
            } else {
                c.child(track_list(s, p, fit, "Coming Up", rest, move |s: &mut App, i| s.play_index(cur + 1 + i)))
            }
        }
    }
}

fn recent_page(s: &App, p: &Pal, fit: Fit) -> El {
    let c = col().w_full().gap(30.0).child(large_title("Recently Played", p));
    if s.history.is_empty() {
        return c.child(text("Songs you play show up here.").font(BODY).color(p.dim));
    }
    c.child(shelf(s, p, "Jump Back In", &s.history, "recent")).child(track_list(
        s,
        p,
        fit,
        "History",
        &s.history,
        |s: &mut App, i| {
            let list = s.history.clone();
            s.play_from(list, i)
        },
    ))
}

fn liked_page(s: &App, p: &Pal, fit: Fit) -> El {
    let liked = &s.taste.liked;
    let mut c = col().w_full().gap(26.0).child(large_title("Liked Songs", p));
    if liked.is_empty() {
        return c.child(text("Tap ♥ on any song (or press L while it plays) and it lands here. Likes also steer your mixes and radio.").font(BODY).color(p.dim).wrap());
    }
    let pill = |key: &'static str, glyph: &'static Icon, label: &'static str, filled: bool, f: fn(&mut App)| {
        row()
            .key(key)
            .h(34.0)
            .px(16.0)
            .gap(7.0)
            .radius(17.0)
            .fill(if filled { p.accent } else { Color::TRANSPARENT })
            .border(if filled { 0.0 } else { 1.0 }, p.text.alpha(0.25))
            .hover(if filled { p.accent.alpha(0.88) } else { p.hover })
            .center_y()
            .press_scale(0.96)
            .on_press(f)
            .child(icon(glyph, 13.0).color(if filled { p.on_accent } else { p.text }))
            .child(text(label).font(TITLE).color(if filled { p.on_accent } else { p.text }))
    };
    c = c.child(
        row()
            .gap(10.0)
            .child(text(format!("{} songs", liked.len())).font(BODY).color(p.dim).pr(8.0))
            .child(pill("liked-play", &PLAY, "Play", true, |s| {
                let l = s.taste.liked.clone();
                s.play_from(l, 0)
            }))
            .child(pill("liked-shuffle", &SHUFFLE, "Shuffle", false, |s| {
                s.shuffle = true;
                let l = s.taste.liked.clone();
                let i = (taste_now_nanos() % l.len() as u64) as usize;
                s.play_from(l, i)
            })),
    );
    c.child(track_list(s, p, fit, "Songs", liked, |s: &mut App, i| {
        let l = s.taste.liked.clone();
        s.play_from(l, i)
    }))
}

fn taste_now_nanos() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos() as u64).unwrap_or(0)
}

// ------------------------------------------------------------------ pieces

/// A horizontal row of big artwork cards.
fn shelf(s: &App, p: &Pal, title: &str, tracks: &[Track], key: &str) -> El {
    let mut cards = row().gap(18.0).pb(6.0);
    for t in tracks {
        let tr = t.clone();
        let playing = s.now_playing().is_some_and(|n| n.id == t.id);
        let play_badge = el()
            .size(34.0, 34.0)
            .radius(17.0)
            .fill(p.accent)
            .center()
            .shadow(4.0, 12.0, Color::BLACK.alpha(0.4))
            .group_opacity(0.0, 1.0)
            .child(icon(&PLAY, 14.0).color(p.on_accent));
        cards = cards.child(
            col()
                .key(format!("{key}-{}", t.id))
                .w(CARD)
                .gap(8.0)
                .hover_group()
                .press_scale(0.98)
                .on_press(move |s: &mut App| s.play_track(tr.clone()))
                .child(
                    cover(s, t, CARD, 8.0, p)
                        .shadow(6.0, 16.0, Color::BLACK.alpha(0.35))
                        .child(el().absolute(CARD - 44.0, CARD - 44.0).child(play_badge)),
                )
                .child(
                    col()
                        .w_full()
                        .gap(2.0)
                        .child(text(t.title.clone()).font(TITLE).color(if playing { p.accent } else { p.text }).truncate())
                        .child(text(t.artist.clone()).font(BODY).color(p.dim).truncate()),
                ),
        );
    }
    col()
        .w_full()
        .gap(12.0)
        .child(text(title.to_string()).font(SECTION).color(p.text))
        .child(cards.scroll_x().w_full())
}

/// A row's ♥ and radio buttons: shown while the row is hovered, the ♥ always once liked.
fn row_actions(s: &App, p: &Pal, t: &Track) -> El {
    let liked = s.taste.is_liked(&t.id);
    let (a, b) = (t.clone(), t.clone());
    row()
        .gap(2.0)
        .center_y()
        .child(
            el().key(format!("radio-{}", t.id))
                .size(28.0, 28.0)
                .radius(14.0)
                .center()
                .hover(p.hover)
                .group_opacity(0.0, 1.0)
                .help("Start song radio")
                .on_press(move |s: &mut App| s.start_radio(a.clone()))
                .child(icon(&RADIO, 15.0).color(p.dim)),
        )
        .child(
            el().key(format!("like-{}", t.id))
                .size(28.0, 28.0)
                .radius(14.0)
                .center()
                .hover(p.hover)
                .group_opacity(if liked { 1.0 } else { 0.0 }, 1.0)
                .on_press(move |s: &mut App| s.toggle_like(&b))
                .child(icon(if liked { &HEART_FILL } else { &HEART }, 15.0).color(if liked { p.accent } else { p.dim })),
        )
}

/// A two-line row for the Songs column beside the top result.
fn compact_row(s: &App, p: &Pal, t: &Track, i: usize) -> El {
    let playing = s.now_playing().is_some_and(|n| n.id == t.id);
    row()
        .key(format!("song-{i}"))
        .w_full()
        .h(65.0)
        .px(8.0)
        .gap(12.0)
        .radius(8.0)
        .center_y()
        .hover(p.hover)
        .hover_group()
        .on_press(move |s: &mut App| s.play_from(s.results.clone(), i))
        .child(cover(s, t, 44.0, 5.0, p))
        .child(
            col()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .h_full()
                .justify_center()
                .gap(3.0)
                .border_sides([0.0, 0.0, 1.0, 0.0], p.line)
                .child(text(t.title.clone()).font(TITLE).color(if playing { p.accent } else { p.text }).truncate())
                .child(text(t.artist.clone()).font(BODY).color(p.dim).truncate()),
        )
        .child(row_actions(s, p, t))
        .child(text(t.duration.clone()).font(NUM).color(p.faint))
}

/// A numbered track list with album and time columns.
fn track_list(s: &App, p: &Pal, fit: Fit, title: &str, tracks: &[Track], on_play: impl Fn(&mut App, usize) + Clone + 'static) -> El {
    let playing = s.now_playing().map(|t| t.id.clone());
    let mut list = col().w_full();
    for (i, t) in tracks.iter().enumerate() {
        let is_playing = playing.as_deref() == Some(t.id.as_str());
        let num: El = if is_playing { icon(&BARS, 13.0).color(p.accent) } else { text(format!("{}", i + 1)).font(NUM).color(p.faint) };
        let f = on_play.clone();
        list = list.child(
            row()
                .key(format!("{title}-{i}-{}", t.id))
                .w_full()
                .h(52.0)
                .px(10.0)
                .gap(14.0)
                .radius(7.0)
                .center_y()
                .fill(if i % 2 == 1 { p.text.alpha(0.025) } else { Color::TRANSPARENT })
                .hover(p.hover)
                .hover_group()
                .on_press(move |s: &mut App| f(s, i))
                .child(el().w(22.0).center().child(num))
                .child(cover(s, t, 36.0, 4.0, p))
                .child(
                    col()
                        .grow(1.0)
                        .shrink()
                        .min_w(0.0)
                        .gap(2.0)
                        .child(text(t.title.clone()).font(TITLE).color(if is_playing { p.accent } else { p.text }).truncate())
                        .child(text(t.artist.clone()).font(SMALL).color(p.dim).truncate()),
                )
                .child(if fit == Fit::Narrow { el() } else { text(t.album.clone()).font(BODY).color(p.dim).w(if fit == Fit::Wide { 220.0 } else { 150.0 }).truncate() })
                .child(row_actions(s, p, t))
                .child(text(t.duration.clone()).font(NUM).color(p.faint).w(40.0)),
        );
    }
    col().w_full().gap(10.0).child(text(title.to_string()).font(SECTION).color(p.text)).child(list)
}

fn cover(s: &App, t: &Track, size: f32, r: f32, p: &Pal) -> El {
    let base = el().size(size, size).radius(r).fill(p.well);
    match s.art.get(&t.id) {
        Some(img) => base.pixels_fit(img.pixels(), SurfaceFit::Fill),
        None => base.center().child(icon(&NOTE, size * 0.36).color(p.faint)),
    }
}

pub fn clock(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

// ------------------------------------------------------------------ settings

fn settings_page(s: &App, p: &Pal) -> El {
    let quality: Vec<(Quality, &'static str, &'static str, bool)> =
        Quality::ALL.into_iter().map(|q| (q, q.name(), q.detail(), q == Quality::Best)).collect();
    let options = option_list(p, "quality", &quality, s.quality, |s, q| s.set_quality(q));

    let st = &s.status;
    let now = if s.current.is_some() && !st.codec.is_empty() {
        let mut parts = vec![st.codec.to_uppercase()];
        if st.rate > 0 {
            parts.push(format!("{:.1} kHz", st.rate as f32 / 1000.0).replace(".0 kHz", " kHz"));
        }
        if st.bitrate > 0.0 {
            parts.push(format!("{} kbps", (st.bitrate / 1000.0).round() as u32));
        }
        parts.join(" · ")
    } else {
        "Nothing playing".to_string()
    };

    col()
        .w_full()
        .gap(26.0)
        .child(large_title("Settings", p))
        .child(
            col()
                .w_full()
                .gap(10.0)
                .child(text("Audio Quality").font(SECTION).color(p.text))
                .child(text("Applies right away. The current song reloads at the same spot.").font(BODY).color(p.dim))
                .child(el().h(4.0))
                .child(options),
        )
        .child(artwork_settings(s, p))
        .child(vocal_settings(s, p))
        .child(
            row()
                .w_full()
                .max_w(620.0)
                .gap(10.0)
                .center_y()
                .child(text("Now streaming").font(BODY).color(p.dim))
                .child(spacer())
                .child(text(now).font(NUM).color(p.text)),
        )
        .child(
            col()
                .w_full()
                .max_w(620.0)
                .gap(8.0)
                .padding(16.0)
                .radius(12.0)
                .border(1.0, p.line)
                .child(text("About lossless").font(TITLE).color(p.text))
                .child(
                    text(
                        "YouTube Music doesn't stream lossless audio. Best is the highest quality it offers. \
                         Encore plays it untouched: no ReplayGain, no volume boost past 100%, and Opus \
                         arrives at 48 kHz, the rate PipeWire already runs at, so nothing gets resampled.",
                    )
                    .font(BODY)
                    .color(p.dim)
                    .wrap(),
                ),
        )
}

// ------------------------------------------------------------------ home

fn greeting() -> &'static str {
    match local_hour() {
        5..=11 => "Good morning",
        12..=16 => "Good afternoon",
        17..=21 => "Good evening",
        _ => "Up late",
    }
}

fn home_page(s: &App, p: &Pal, fit: Fit) -> El {
    let h = &s.home;
    let mut c = col().w_full().gap(34.0).child(
        col()
            .gap(2.0)
            .child(text(format!("> {}_", greeting().to_lowercase())).font(TextStyle::new(12.5, 600.0).mono()).color(p.accent))
            .child(large_title("Home", p)),
    );

    c = c.child(hero(s, p, fit));

    // Top Picks: three big cards.
    let mut picks = row().gap(18.0).pb(8.0);
    let mut n = 0;
    for (k, (seed, mix)) in h.mixes.iter().enumerate().take(3) {
        let mut artists: Vec<&str> = Vec::new();
        for t in mix {
            if !artists.contains(&t.artist.as_str()) {
                artists.push(&t.artist);
            }
            if artists.len() == 3 {
                break;
            }
        }
        let (seed2, mix2) = (seed.clone(), mix.clone());
        picks = picks.child(feature_card(
            s,
            p,
            ["mix-1", "mix-2", "mix-3"][k],
            ["DAILY MIX 1", "DAILY MIX 2", "DAILY MIX 3"][k],
            &format!("{} Mix", seed.artist),
            &artists.join(", "),
            Some(seed),
            [18.0, 265.0, 150.0][k],
            move |s: &mut App| s.start_station(seed2.clone(), mix2.clone()),
        ));
        n += 1;
    }
    if let Some(last) = s.history.first().filter(|_| n == 0) {
        if !h.picks.is_empty() {
            let seed = last.clone();
            picks = picks.child(feature_card(
                s,
                p,
                "feature-station",
                "STATION",
                &format!("{} Radio", seed.artist),
                &format!("Songs like “{}”", seed.title),
                h.picks.first(),
                265.0,
                move |s: &mut App| {
                    let picks = s.home.picks.clone();
                    s.start_station(seed.clone(), picks)
                },
            ));
            n += 1;
        }
    }
    for (key, label, title, list, hue) in [
        ("feature-hits", "TODAY'S HITS", "Top Songs Right Now", &h.hits, 330.0),
        ("feature-chill", "MOOD", "Chill Vibes", &h.chill, 200.0),
        ("feature-good", "MOOD", "Feel-Good Hits", &h.feel_good, 85.0),
    ] {
        if n >= 3 || list.is_empty() {
            continue;
        }
        let mut artists: Vec<&str> = Vec::new();
        for t in list.iter() {
            if !artists.contains(&t.artist.as_str()) {
                artists.push(&t.artist);
            }
            if artists.len() == 3 {
                break;
            }
        }
        let sub = artists.join(", ");
        picks = picks.child(feature_card(s, p, key, label, title, &sub, list.first(), hue, move |s: &mut App| {
            let l = match key {
                "feature-hits" => s.home.hits.clone(),
                "feature-chill" => s.home.chill.clone(),
                _ => s.home.feel_good.clone(),
            };
            s.play_from(l, 0)
        }));
        n += 1;
    }
    if n > 0 {
        c = c.child(section(if h.mixes.is_empty() { "Top Picks for You" } else { "Made for You" }, picks.scroll_x().w_full(), p));
    } else {
        c = c.child(loading_card(p));
    }

    // Quick Picks: columns of compact rows, scrolling sideways.
    if !h.picks.is_empty() {
        let title = match &h.seed {
            Some(t) => format!("Because you played {}", t.artist),
            None => "Quick Picks".to_string(),
        };
        let per_col = 4;
        let col_w = if fit == Fit::Narrow { 300.0 } else { 340.0 };
        let mut grid = row().gap(16.0).pb(4.0);
        for (ci, chunk) in h.picks.chunks(per_col).take(4).enumerate() {
            let mut column = col().w(col_w).gap(2.0);
            for (ri, t) in chunk.iter().enumerate() {
                let i = ci * per_col + ri;
                column = column.child(pick_row(s, p, t, i));
            }
            grid = grid.child(column);
        }
        c = c.child(section(&title, grid.scroll_x().w_full(), p));
    }

    if !s.history.is_empty() {
        c = c.child(shelf(s, p, "Recently Played", &s.history[..s.history.len().min(16)], "home-recent"));
    }
    if !h.hits.is_empty() {
        c = c.child(shelf(s, p, "Today's Hits", &h.hits, "home-hits"));
    }

    // Browse by mood: gradient tiles.
    let cols = match fit {
        Fit::Wide => 4,
        Fit::Medium => 4,
        Fit::Narrow => 2,
    };
    let mut tiles = col().w_full().gap(14.0);
    for chunk in MOODS.chunks(cols) {
        let mut r = row().w_full().gap(14.0);
        for &(name, query, hue) in chunk {
            r = r.child(
                el().key(format!("mood-{name}"))
                    .w(0.0)
                    .grow(1.0)
                    .h(92.0)
                    .radius(12.0)
                    .gradient(oklch(0.58, 0.15, hue), oklch(0.40, 0.13, hue + 25.0))
                    .padding(16.0)
                    .hover_group()
                    .press_scale(0.97)
                    .on_press(move |s: &mut App| s.search_for(query))
                    .clip_rounded(12.0)
                    .child(match &s.dots {
                        Some(img) => el().absolute(0.0, 0.0).w_full().h(92.0).pixels_fit(img.pixels(), SurfaceFit::Fill),
                        None => el(),
                    })
                    .child(text(name).font(TextStyle::new(17.0, 750.0).tracking(-0.2)).color(Color::WHITE))
                    .child(spacer())
                    .child(text("> play mix_").font(NUM).color(Color::WHITE.alpha(0.8)).group_opacity(0.0, 1.0)),
            );
        }
        tiles = tiles.child(r);
    }
    c = c.child(section("Browse by Mood", tiles, p));

    if !h.chill.is_empty() {
        c = c.child(shelf(s, p, "Chill Vibes", &h.chill, "home-chill"));
    }
    if !h.feel_good.is_empty() {
        c = c.child(shelf(s, p, "Feel-Good Hits", &h.feel_good, "home-good"));
    }
    c
}

/// The shared dot texture laid over a card of known size.
fn dots_over(s: &App, w: f32, h: f32) -> El {
    match &s.dots {
        Some(img) => el().absolute(0.0, 0.0).size(w, h).pixels_fit(img.pixels(), SurfaceFit::Fill),
        None => el(),
    }
}

fn section(title: &str, body: El, p: &Pal) -> El {
    col().w_full().gap(12.0).child(text(title.to_string()).font(SECTION).color(p.text)).child(body)
}

fn loading_card(p: &Pal) -> El {
    row()
        .w_full()
        .h(120.0)
        .radius(14.0)
        .fill(p.card)
        .center()
        .gap(10.0)
        .child(icon(&RADIO, 18.0).color(p.faint))
        .child(text("Finding music for you…").font(BODY).color(p.dim))
}

/// A tall, colourful card: a label, big artwork, a title and a line under it.
#[allow(clippy::too_many_arguments)]
fn feature_card(
    s: &App,
    p: &Pal,
    key: &str,
    label: &str,
    title: &str,
    sub: &str,
    art: Option<&Track>,
    hue: f32,
    f: impl Fn(&mut App) + 'static,
) -> El {
    const W: f32 = 268.0;
    let artwork = match art {
        Some(t) => cover(s, t, W - 36.0, 10.0, p).shadow(10.0, 26.0, Color::BLACK.alpha(0.45)),
        None => el().size(W - 36.0, W - 36.0).radius(10.0).fill(Color::BLACK.alpha(0.2)).center().child(icon(&NOTE, 64.0).color(Color::WHITE.alpha(0.5))),
    };
    let play = el()
        .size(44.0, 44.0)
        .radius(22.0)
        .fill(Color::WHITE)
        .center()
        .shadow(4.0, 14.0, Color::BLACK.alpha(0.35))
        .group_opacity(0.0, 1.0)
        .child(icon(&PLAY, 18.0).color(oklch(0.25, 0.0, 0.0)));
    col()
        .key(key.to_string())
        .w(W)
        .h(W + 104.0)
        .padding(18.0)
        .gap(12.0)
        .radius(16.0)
        .gradient(oklch(0.50, 0.13, hue), oklch(0.27, 0.07, hue + 20.0))
        .hover_group()
        .press_scale(0.98)
        .on_press(f)
        .child(dots_over(s, W, W + 104.0))
        .child(text(format!("▚ {label}")).font(MONO_LABEL).color(Color::WHITE.alpha(0.85)))
        .child(artwork.child(el().absolute(W - 36.0 - 56.0, W - 36.0 - 56.0).child(play)))
        .child(
            col()
                .w_full()
                .gap(3.0)
                .child(text(title.to_string()).font(TextStyle::new(17.0, 720.0).tracking(-0.2)).color(Color::WHITE).truncate())
                .child(text(sub.to_string()).font(BODY).color(Color::WHITE.alpha(0.72)).truncate()),
        )
}

/// One row of Quick Picks: plays the picks from this song on.
fn pick_row(s: &App, p: &Pal, t: &Track, i: usize) -> El {
    let playing = s.now_playing().is_some_and(|n| n.id == t.id);
    row()
        .key(format!("pick-{i}-{}", t.id))
        .w_full()
        .h(56.0)
        .px(6.0)
        .gap(12.0)
        .radius(8.0)
        .center_y()
        .hover(p.hover)
        .hover_group()
        .on_press(move |s: &mut App| {
            let list = s.home.picks.clone();
            s.play_from(list, i)
        })
        .child(cover(s, t, 44.0, 5.0, p))
        .child(
            col()
                .grow(1.0)
                .shrink()
                .min_w(0.0)
                .gap(3.0)
                .child(text(t.title.clone()).font(TITLE).color(if playing { p.accent } else { p.text }).truncate())
                .child(text(t.artist.clone()).font(SMALL).color(p.dim).truncate()),
        )
        .child(row_actions(s, p, t))
        .child(text(t.duration.clone()).font(NUM).color(p.faint))
}

// ------------------------------------------------------------------ hero (ASCII)

/// Shown in the hero before anything has played.
const BANNER: [&str; 6] = [
    " ███████ ███    ██  ██████  ██████  ██████  ███████ ",
    " ██      ████   ██ ██      ██    ██ ██   ██ ██      ",
    " █████   ██ ██  ██ ██      ██    ██ ██████  █████   ",
    " ██      ██  ██ ██ ██      ██    ██ ██   ██ ██      ",
    " ███████ ██   ████  ██████  ██████  ██   ██ ███████ ",
    "                                                    ",
];

/// The big banner at the top of Home: the song you're on (or were last on) drawn in ASCII,
/// with its name and a way back into it.
fn hero(s: &App, p: &Pal, fit: Fit) -> El {
    let playing = s.now_playing();
    let track = playing.or(s.history.first());
    let ascii_lines: Vec<String> = match track.and_then(|t| s.ascii.get(&t.id)) {
        Some(lines) => lines.clone(),
        None => BANNER.iter().map(|l| l.to_string()).collect(),
    };
    let rows = ascii_lines.len().max(1) as f32;
    let mut art = col().gap(0.0);
    for (i, line) in ascii_lines.into_iter().enumerate() {
        // Fade from accent to paper down the picture, like a CRT's phosphor glow.
        let t = i as f32 / rows;
        let ink = oklch(0.68 + 0.22 * t, p.chroma.max(0.08) * (1.0 - t * 0.85), p.hue + 40.0 * t);
        art = art.child(text(line).font(ASCII).color(ink));
    }
    let art = el().padding(14.0).radius(10.0).fill(Color::BLACK.alpha(0.55)).border(1.0, p.line).child(art);

    let (label, title, sub) = match (playing, track) {
        (Some(t), _) => ("NOW PLAYING", t.title.clone(), t.artist.clone()),
        (None, Some(t)) => ("PICK UP WHERE YOU LEFT OFF", t.title.clone(), t.artist.clone()),
        _ => ("WELCOME", "Your music, in dots.".to_string(), "Search, or pick a mood below to start.".to_string()),
    };

    let mut buttons = row().gap(10.0).pt(6.0);
    if let Some(t) = track.cloned() {
        let resume = playing.is_none();
        let t2 = t.clone();
        buttons = buttons
            .child(
                row()
                    .key("hero-play")
                    .h(34.0)
                    .px(16.0)
                    .gap(7.0)
                    .radius(17.0)
                    .fill(p.accent)
                    .center_y()
                    .press_scale(0.96)
                    .on_press(move |s: &mut App| if resume { s.play_track(t.clone()) } else { s.toggle() })
                    .child(icon(if resume || s.status.paused { &PLAY } else { &PAUSE }, 13.0).color(p.on_accent))
                    .child(text(if resume { "Play" } else if s.status.paused { "Resume" } else { "Pause" }).font(TITLE).color(p.on_accent)),
            )
            .child(
                row()
                    .key("hero-radio")
                    .h(34.0)
                    .px(16.0)
                    .gap(7.0)
                    .radius(17.0)
                    .border(1.0, p.text.alpha(0.25))
                    .hover(p.hover)
                    .center_y()
                    .press_scale(0.96)
                    .on_press(move |s: &mut App| s.start_radio(t2.clone()))
                    .child(icon(&RADIO, 14.0).color(p.text))
                    .child(text("Start Radio").font(TITLE).color(p.text)),
            );
    }

    let words = col()
        .w(0.0)
        .grow(1.0)
        .min_w(0.0)
        .gap(6.0)
        .child(text(format!("▚ {label}")).font(MONO_LABEL).color(p.accent))
        .child(text(title).font(TextStyle::new(if fit == Fit::Narrow { 24.0 } else { 34.0 }, 780.0).tracking(-0.8)).color(p.text).truncate())
        .child(text(sub).font(TextStyle::new(15.0, 500.0)).color(p.dim).truncate())
        .child(buttons);

    let body = if fit == Fit::Narrow {
        col().w_full().gap(20.0).child(art).child(words.w_full())
    } else {
        row().w_full().gap(32.0).center_y().child(art).child(words)
    };

    el().key("hero")
        .w_full()
        .padding(24.0)
        .radius(18.0)
        .gradient(p.accent.alpha(0.22), p.text.alpha(0.03))
        .border(1.0, p.line)
        .clip_rounded(18.0)
        .child(match &s.dots {
            Some(img) => el().absolute(0.0, 0.0).w_full().h_full().opacity(0.6).pixels_fit(img.pixels(), SurfaceFit::Fill),
            None => el(),
        })
        .child(body)
}

// ------------------------------------------------------------------ settings: option lists

/// A card of radio-style options, like iOS settings.
fn option_list<T: Copy + PartialEq + 'static>(
    p: &Pal,
    key: &str,
    items: &[(T, &'static str, &'static str, bool)],
    current: T,
    pick: fn(&mut App, T),
) -> El {
    let mut options = col().w_full().max_w(620.0).radius(12.0).fill(p.card).clip_rounded(12.0);
    for (n, &(value, name, detail, recommended)) in items.iter().enumerate() {
        let on = current == value;
        options = options.child(
            row()
                .key(format!("{key}-{name}"))
                .w_full()
                .px(18.0)
                .py(14.0)
                .gap(14.0)
                .center_y()
                .hover(p.hover)
                .border_sides([if n == 0 { 0.0 } else { 1.0 }, 0.0, 0.0, 0.0], p.line)
                .on_press(move |s: &mut App| pick(s, value))
                .child(
                    el().size(22.0, 22.0)
                        .radius(11.0)
                        .fill(if on { p.accent } else { Color::TRANSPARENT })
                        .border(if on { 0.0 } else { 1.5 }, p.faint)
                        .center()
                        .child(if on { icon(&CHECK, 13.0).color(p.on_accent) } else { el() }),
                )
                .child(
                    col()
                        .grow(1.0)
                        .shrink()
                        .gap(3.0)
                        .child(
                            row()
                                .gap(8.0)
                                .center_y()
                                .child(text(name).font(TITLE).color(p.text))
                                .child(if recommended {
                                    el().h(17.0).px(6.0).radius(4.0).fill(p.accent_soft).center().child(
                                        text("RECOMMENDED").font(TextStyle::new(9.0, 750.0).tracking(0.6)).color(p.accent),
                                    )
                                } else {
                                    el()
                                }),
                        )
                        .child(text(detail).font(BODY).color(p.dim).wrap()),
                ),
        );
    }
    options
}

fn artwork_settings(s: &App, p: &Pal) -> El {
    let items: Vec<(Style, &'static str, &'static str, bool)> =
        Style::ALL.into_iter().map(|st| (st, st.name(), st.detail(), false)).collect();
    // A live preview: the current (or last) cover in the chosen style.
    let preview = match s.now_playing().or(s.history.first()) {
        Some(t) => cover(s, t, 96.0, 10.0, p),
        None => el(),
    };
    col()
        .w_full()
        .gap(10.0)
        .child(text("Artwork").font(SECTION).color(p.text))
        .child(text("How covers are drawn everywhere in Encore.").font(BODY).color(p.dim))
        .child(el().h(4.0))
        .child(
            row()
                .w_full()
                .max_w(620.0)
                .gap(18.0)
                .child(option_list(p, "art", &items, s.art_style, |s, v| s.set_art_style(v)).w(0.0).grow(1.0))
                .child(preview),
        )
}

// ------------------------------------------------------------------ lyrics

const LYRIC: TextStyle = TextStyle::new(28.0, 760.0).tracking(-0.5).lh(36.0);
const LYRIC_NARROW: TextStyle = TextStyle::new(22.0, 760.0).tracking(-0.4).lh(29.0);

fn lyrics_page(s: &App, p: &Pal, fit: Fit) -> El {
    let Some(t) = s.now_playing() else {
        return col()
            .w_full()
            .gap(10.0)
            .child(large_title("Lyrics", p))
            .child(text("Play a song and its lyrics show up here, in time with the music.").font(BODY).color(p.dim));
    };
    let head = row()
        .w_full()
        .gap(14.0)
        .center_y()
        .child(cover(s, t, 56.0, 8.0, p))
        .child(
            col()
                .w(0.0)
                .grow(1.0)
                .gap(3.0)
                .child(text("▚ LYRICS").font(MONO_LABEL).color(p.accent))
                .child(text(t.title.clone()).font(TextStyle::new(17.0, 720.0)).color(p.text).truncate())
                .child(text(t.artist.clone()).font(BODY).color(p.dim).truncate()),
        )
        .child(
            row()
                .key("float-lyrics")
                .h(32.0)
                .px(14.0)
                .gap(7.0)
                .radius(16.0)
                .fill(if s.float_lyrics { p.accent } else { Color::TRANSPARENT })
                .border(if s.float_lyrics { 0.0 } else { 1.0 }, p.text.alpha(0.25))
                .hover(if s.float_lyrics { p.accent.alpha(0.88) } else { p.hover })
                .center_y()
                .press_scale(0.96)
                .on_press(|s: &mut App| s.toggle_float_lyrics())
                .child(icon(&QUOTE, 13.0).color(if s.float_lyrics { p.on_accent } else { p.text }))
                .child(text(if s.float_lyrics { "Floating" } else { "Float in Corner" }).font(TITLE).color(if s.float_lyrics { p.on_accent } else { p.text })),
        );
    let style = if fit == Fit::Narrow { LYRIC_NARROW } else { LYRIC };
    let body: El = match s.lyrics.get(&t.id) {
        None | Some(Lyrics::Loading) => text("Finding lyrics…").font(style).color(p.faint),
        Some(Lyrics::Missing) => col()
            .gap(8.0)
            .child(text("No lyrics for this one.").font(style).color(p.faint))
            .child(text("LRCLIB doesn't have this song yet.").font(BODY).color(p.dim)),
        Some(Lyrics::Plain(lines)) => {
            let mut c = col().w_full().gap(10.0).child(text("Not synced: the full text, without timing.").font(SMALL).color(p.faint).pb(8.0));
            for (i, line) in lines.iter().enumerate() {
                c = c.child(text(if line.is_empty() { " ".to_string() } else { line.clone() }).key(format!("lyric-{i}")).font(style).color(p.dim).wrap());
            }
            c
        }
        Some(Lyrics::Synced(lines)) => {
            let now = s.lyric_line;
            let mut c = col().w_full().gap(14.0);
            for (i, (at, line)) in lines.iter().enumerate() {
                let at = *at;
                let ink = match now {
                    Some(n) if n == i => p.text,
                    Some(n) if i < n => p.text.alpha(0.28),
                    _ => p.text.alpha(0.45),
                };
                let shown = if line.is_empty() { "♪".to_string() } else { line.clone() };
                c = c.child(
                    text(shown)
                        .key(format!("lyric-{i}"))
                        .font(style)
                        .color(ink)
                        .wrap()
                        .motion(Spring::new(0.25, 0.0))
                        .on_press(move |s: &mut App| {
                            if let Some(pl) = &mut s.player {
                                pl.seek(at, true);
                            }
                        }),
                );
            }
            c.child(el().h(200.0))
        }
    };
    col().w_full().max_w(760.0).gap(30.0).child(head).child(body)
}

fn vocal_settings(s: &App, p: &Pal) -> El {
    use crate::separate::Engine;
    let items = [
        (Engine::Fast, "Fast", "Spleeter, int8: a whole song in about 20 seconds. Clean vocals, some artifacts.", true),
        (Engine::Hq, "Best", "Plays the Fast stems first, then upgrades to Kim_Vocal_2 (UVR's MDX-Net) when it finishes. Studio-clean, but slower than the song itself on this CPU.", false),
    ];
    let mut c = col()
        .w_full()
        .gap(10.0)
        .child(text("AI Vocals").font(SECTION).color(p.text))
        .child(text("Vocals and Karaoke (🎤, or K) split songs with an AI model on this computer. The next song in the queue gets split ahead of time.").font(BODY).color(p.dim).wrap())
        .child(el().h(4.0))
        .child(option_list(p, "engine", &items, s.engine, |s, e| s.set_engine(e)));
    if let Some(e) = &s.stem_error {
        c = c.child(text(format!("Last split failed: {e}")).font(SMALL).color(p.accent).wrap());
    }
    c
}
