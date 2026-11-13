//! Application state, queue transitions, background deliveries, and window setup.
mod radio;
mod ui;
mod vocals;

use crate::storage::{config_dir, load_history, save_history, setting, settings_path};
use crate::{art, float, lyrics, mpv, separate, taste, theme, ytm};

use rune::{AppSpec, Image, ImageCache, Key, KeyEvent, Latest, TextEdit, Tx, Waker};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use ytm::Track;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Home,
    Search,
    Queue,
    Recent,
    Liked,
    Lyrics,
    Settings,
}

/// What you hear of the mix. Real-time and approximate: it works on where sounds sit in the
/// stereo field, so centred drums and bass bleed through "Vocals".
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Vocal {
    Normal,
    Vocals,
    Karaoke,
}

impl Vocal {
    pub fn next(self) -> Vocal {
        match self {
            Vocal::Normal => Vocal::Vocals,
            Vocal::Vocals => Vocal::Karaoke,
            Vocal::Karaoke => Vocal::Normal,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Vocal::Normal => "Normal",
            Vocal::Vocals => "Vocals",
            Vocal::Karaoke => "Karaoke",
        }
    }
    /// The ffmpeg chain: keep the centre (where lead vocals sit) and the voice band, or take
    /// the centre out.
    pub(crate) fn filter(self) -> &'static str {
        match self {
            Vocal::Normal => "",
            Vocal::Vocals => {
                "lavfi=[stereotools=slev=0.015625,highpass=f=140,lowpass=f=8500,equalizer=f=2800:t=q:w=1.4:g=3,volume=1.6]"
            }
            Vocal::Karaoke => "lavfi=[stereotools=mlev=0.015625,volume=1.8]",
        }
    }
}

/// Video in mpv's window: up to 1080p, any codec mpv plays.
/// Premium-only streams are left out: they fail without a YouTube Premium login.
const VIDEO_FORMAT: &str =
    "bestvideo[height<=?1080][format_note!*=Premium]+bestaudio/best[format_note!*=Premium]/best";

/// Streaming quality. YouTube Music tops out at Opus ~160 kbps; there is no lossless tier.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Best,
    Standard,
    Saver,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Best, Quality::Standard, Quality::Saver];

    /// The yt-dlp format selector, with fallbacks so a song never fails over a missing format.
    pub fn format(self) -> &'static str {
        match self {
            Quality::Best => "251/bestaudio[acodec=opus]/bestaudio/best",
            Quality::Standard => "140/bestaudio[ext=m4a]/bestaudio/best",
            Quality::Saver => "250/249/139/worstaudio/worst",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Quality::Best => "Best",
            Quality::Standard => "Standard",
            Quality::Saver => "Data Saver",
        }
    }
    pub fn detail(self) -> &'static str {
        match self {
            Quality::Best => {
                "Opus · 48 kHz · ~130–160 kbps. The highest quality YouTube Music streams."
            }
            Quality::Standard => "AAC · 44.1 kHz · 128 kbps. Slightly smaller, widely compatible.",
            Quality::Saver => "Opus · ~50–70 kbps. Uses about half the data, for slow connections.",
        }
    }
    fn id(self) -> &'static str {
        match self {
            Quality::Best => "best",
            Quality::Standard => "standard",
            Quality::Saver => "saver",
        }
    }
}

/// Moods for Home's tiles: name, the search it runs, and a hue for its gradient.
pub const MOODS: [(&str, &str, f32); 8] = [
    ("Chill", "chill songs", 210.0),
    ("Workout", "workout songs", 30.0),
    ("Focus", "focus instrumental music", 265.0),
    ("Party", "party hits", 330.0),
    ("Feel Good", "feel good songs", 95.0),
    ("Romance", "love songs", 5.0),
    ("Hip-Hop", "hip hop hits", 55.0),
    ("Lo-fi", "lofi beats", 175.0),
];

/// Home's shelves, filled from background requests after launch.
#[derive(Default)]
pub struct Home {
    /// Songs like the last one played (or today's hits on a first run).
    pub picks: Vec<Track>,
    /// The song the picks come from, when they're personal.
    pub seed: Option<Track>,
    pub hits: Vec<Track>,
    pub chill: Vec<Track>,
    pub feel_good: Vec<Track>,
    /// Daily Mixes: a seed from your top songs, and the mix built from it.
    pub mixes: Vec<(Track, Vec<Track>)>,
    pub loaded: bool,
}

pub enum Msg {
    Picks(Option<Track>, Vec<Track>),
    Hits(Vec<Track>),
    Chill(Vec<Track>),
    FeelGood(Vec<Track>),
    Mixes(Vec<(Track, Vec<Track>)>),
    Lyrics(String, lyrics::Lyrics),
    /// The music video found (or not) for this song id.
    Video(String, Option<String>),
    StemProgress(String, f32),
    StemDone(String, separate::Engine, Result<separate::Stems, String>),
    /// Songs for the station with this epoch.
    Station(u64, Result<Vec<Track>, String>),
}

/// The local hour (0–23), for the greeting.
pub fn local_hour() -> u32 {
    // SAFETY: localtime_r writes only into `tm`.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        tm.tm_hour as u32
    }
}

fn load_quality() -> Quality {
    let id = setting("quality").unwrap_or_default();
    Quality::ALL
        .into_iter()
        .find(|q| q.id() == id)
        .unwrap_or(Quality::Best)
}

/// One processed cover landing from a worker: id, the fetched bytes, the generation of the
/// style it was drawn in, and the result.
type ArtDelivery = (String, Arc<[u8]>, u64, Option<art::Processed>);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Off,
    All,
    One,
}

pub struct App {
    page: Page,
    query: TextEdit,
    search: Latest<Result<Vec<Track>, String>>,
    /// The query the current results answer.
    searched: String,
    results: Vec<Track>,
    message: Option<String>,

    /// What plays, in order, and where we are in it.
    queue: Vec<Track>,
    current: Option<usize>,
    history: Vec<Track>,
    shuffle: bool,
    repeat: Repeat,
    volume: f32,
    quality: Quality,
    art_style: art::Style,
    /// Bumped on every style change, so covers drawn in an old style are dropped.
    art_gen: u64,
    art_raw: HashMap<String, Arc<[u8]>>,
    ascii: HashMap<String, Vec<String>>,
    dots: Option<Image>,
    /// Where the pointer holds the progress bar while scrubbing (0..1).
    scrub: Option<f32>,

    home: Home,
    inbox: Arc<Mutex<Vec<Msg>>>,
    /// Play on as soon as the station's next songs land (the queue already ran out).
    resume_on_radio: bool,
    station: Option<radio::Station>,
    station_epoch: u64,
    taste: taste::Taste,
    lyrics: HashMap<String, lyrics::Lyrics>,
    lyric_line: Option<usize>,
    /// Where the Lyrics button goes back to.
    prev_page: Page,
    video: bool,
    /// Lyrics floating in a screen corner, over every app.
    float_lyrics: bool,
    float_last: Option<(String, Option<usize>)>,
    video_closed_seen: u64,
    vocal: Vocal,
    /// AI separator for Vocals and Karaoke.
    engine: separate::Engine,
    /// Separations in flight ("id.fast" → progress 0..1).
    stem_jobs: HashMap<String, f32>,
    /// The song whose stem is loaded in place of its stream.
    stem_playing: Option<String>,
    stem_error: Option<String>,
    /// The theme generation the window was last drawn with.
    theme_seen: u64,
    /// Whether the current song's outcome is already in `taste`.
    current_recorded: bool,

    player: Option<mpv::Mpv>,
    status: mpv::Status,
    ended_seen: u64,
    demo: bool,

    waker: Option<Waker>,
    images: Option<ImageCache>,
    art: HashMap<String, Image>,
    art_requested: HashSet<String>,
    /// Covers in the order they landed, oldest first, for `trim_art`.
    art_order: VecDeque<String>,
    art_inbox: Arc<Mutex<Vec<ArtDelivery>>>,
}

fn init() -> App {
    let query = TextEdit::new("");
    App {
        page: Page::Home,
        query,
        search: Latest::default(),
        searched: String::new(),
        results: Vec::new(),
        message: None,
        queue: Vec::new(),
        current: None,
        history: load_history(),
        home: Home::default(),
        inbox: Arc::new(Mutex::new(Vec::new())),
        resume_on_radio: false,
        station: None,
        station_epoch: 0,
        taste: config_dir()
            .map(|d| taste::Taste::load(&d.join("taste.json")))
            .unwrap_or_default(),
        current_recorded: true,
        theme_seen: 0,
        lyrics: HashMap::new(),
        lyric_line: None,
        prev_page: Page::Home,
        video: false,
        float_lyrics: false,
        float_last: None,
        video_closed_seen: 0,
        vocal: Vocal::Normal,
        engine: separate::Engine::from_id(&setting("engine").unwrap_or_default()),
        stem_jobs: HashMap::new(),
        stem_playing: None,
        stem_error: None,
        shuffle: false,
        repeat: Repeat::Off,
        volume: 80.0,
        quality: load_quality(),
        art_style: art::Style::from_id(&setting("art").unwrap_or_default()),
        art_gen: 0,
        art_raw: HashMap::new(),
        ascii: HashMap::new(),
        dots: None,
        scrub: None,
        player: None,
        status: mpv::Status {
            paused: true,
            ..Default::default()
        },
        ended_seen: 0,
        demo: false,
        waker: None,
        images: None,
        art: HashMap::new(),
        art_requested: HashSet::new(),
        art_order: VecDeque::new(),
        art_inbox: Arc::new(Mutex::new(Vec::new())),
    }
}

fn setup(s: &mut App, waker: Waker) {
    theme::follow(waker.clone());
    s.search.set_waker(waker.clone());
    let cache = ImageCache::new(waker.clone());
    s.dots = Some(cache.rgba(512, 512, art::dot_texture(512, 16)));
    s.images = Some(cache);
    match mpv::Mpv::spawn(waker.clone()) {
        Ok(mut p) => {
            p.set_volume(s.volume as f64);
            s.player = Some(p);
        }
        Err(e) => s.message = Some(e),
    }
    s.waker = Some(waker);
    s.load_home();
}

impl App {
    /// Run `work` on a thread and land its message in `poll`.
    fn fetch(&self, work: impl FnOnce() -> Option<Msg> + Send + 'static) {
        let (inbox, waker) = (self.inbox.clone(), self.waker.clone());
        std::thread::spawn(move || {
            if let Some(m) = work() {
                inbox.lock().unwrap().push(m);
                if let Some(w) = waker {
                    w();
                }
            }
        });
    }

    fn load_home(&mut self) {
        let recent: Vec<Track> = self.history.iter().take(16).cloned().collect();
        self.want_art(&recent);
        let seeds = {
            let mut seeds = self.taste.seeds(3);
            if seeds.is_empty() {
                seeds.extend(self.history.first().cloned());
            }
            seeds
        };
        let taste = self.taste.clone();
        let avoid: HashSet<String> = self.history.iter().take(20).map(|t| t.id.clone()).collect();
        let (inbox, waker) = (self.inbox.clone(), self.waker.clone());
        std::thread::spawn(move || {
            let send = |m: Msg| {
                inbox.lock().unwrap().push(m);
                if let Some(w) = &waker {
                    w();
                }
            };
            if seeds.is_empty() {
                // First run: a station from a real current hit, not a search page of filler.
                let Some(hit) = ytm::search_clean("billboard hot 100")
                    .ok()
                    .and_then(|l| l.into_iter().next())
                else {
                    return;
                };
                let list = taste.rank(&[ytm::radio(&hit.id).unwrap_or_default()], &avoid, 40);
                send(Msg::Picks(None, list));
                return;
            }
            // One radio per seed, fetched side by side.
            let lists: Vec<Vec<Track>> = std::thread::scope(|sc| {
                let handles: Vec<_> = seeds
                    .iter()
                    .map(|t| sc.spawn(|| ytm::radio(&t.id).unwrap_or_default()))
                    .collect();
                handles
                    .into_iter()
                    .map(|h| h.join().unwrap_or_default())
                    .collect()
            });
            // Quick Picks: all the radios blended and ranked by your taste.
            send(Msg::Picks(
                seeds.first().cloned(),
                taste.rank(&lists, &avoid, 40),
            ));
            // Daily Mixes: each radio on its own, so every mix keeps its own sound.
            let mixes: Vec<(Track, Vec<Track>)> = seeds
                .iter()
                .cloned()
                .zip(lists.iter())
                .map(|(seed, list)| {
                    let mut l = taste.rank(std::slice::from_ref(list), &avoid, 30);
                    l.retain(|t| t.id != seed.id);
                    (seed, l)
                })
                .filter(|(_, l)| l.len() >= 5)
                .collect();
            if !mixes.is_empty() {
                send(Msg::Mixes(mixes));
            }
        });
        self.fetch(|| Some(Msg::Hits(ytm::search_clean("top hits 2026").ok()?)));
        self.fetch(|| Some(Msg::Chill(ytm::search_clean("chill vibes songs").ok()?)));
        self.fetch(|| Some(Msg::FeelGood(ytm::search_clean("feel good hits").ok()?)));
    }

    fn save_taste(&self) {
        if let Some(d) = config_dir() {
            self.taste.save(&d.join("taste.json"));
        }
    }

    fn search_for(&mut self, q: &str) {
        self.query.replace_text(q);
        self.run_search();
    }

    fn run_search(&mut self) {
        let q = self.query.text.trim().to_string();
        if q.is_empty() {
            return;
        }
        self.page = Page::Search;
        self.message = None;
        self.search.request(q, |q| ytm::search_clean(&q));
        self.searched = self.query.text.trim().to_string();
    }

    fn play_from(&mut self, list: Vec<Track>, index: usize) {
        if index >= list.len() {
            return;
        }
        // Record the outgoing track before replacing the queue it belongs to.
        self.note_outcome(taste::Outcome::judge(
            self.status.time,
            self.status.duration,
            false,
        ));
        self.current = None;
        self.resume_on_radio = false;
        // You picked a list: that ends any station.
        self.stop_radio();
        self.queue = list;
        if self.shuffle {
            // Keep the chosen song first, shuffle what follows it.
            let chosen = self.queue.remove(index);
            shuffle(&mut self.queue);
            self.queue.insert(0, chosen);
            self.play_index(0);
        } else {
            self.play_index(index);
        }
    }

    fn play_track(&mut self, t: Track) {
        self.play_from(vec![t], 0);
    }

    fn play_index(&mut self, i: usize) {
        let Some(track) = self.queue.get(i).cloned() else {
            return;
        };
        // Moving on from a song: learn whether it was a skip or a listen.
        if self.current.is_some() {
            self.note_outcome(taste::Outcome::judge(
                self.status.time,
                self.status.duration,
                false,
            ));
        }
        self.current_recorded = false;
        self.current = Some(i);
        self.want_art(std::slice::from_ref(&track));
        self.history.retain(|h| h.id != track.id);
        self.history.insert(0, track.clone());
        self.history.truncate(48);
        save_history(&self.history);
        // Running low: line up what comes next now, so there's no gap.
        if self.station_low(i) {
            self.refill_station();
        } else if i + 1 == self.queue.len() && self.repeat == Repeat::Off {
            self.autoplay();
        }
        self.lyric_line = None;
        if !self.lyrics.contains_key(&track.id) {
            self.lyrics
                .insert(track.id.clone(), lyrics::Lyrics::Loading);
            let t = track.clone();
            self.fetch(move || Some(Msg::Lyrics(t.id.clone(), lyrics::fetch(&t))));
        }
        if self.video {
            // Find the real music video first; it lands in `poll` and starts there.
            self.status.loading = true;
            let t = track.clone();
            self.fetch(move || Some(Msg::Video(t.id.clone(), ytm::find_video(&t))));
        } else {
            self.status.time = 0.0;
            self.stem_playing = None;
            self.apply_vocal(true);
        }
    }

    /// Lyrics on or off: a page, with the button taking you back where you were.
    fn toggle_lyrics(&mut self) {
        if self.page == Page::Lyrics {
            self.page = self.prev_page;
        } else {
            self.prev_page = self.page;
            self.page = Page::Lyrics;
            self.lyric_line = None;
        }
    }

    fn toggle_video(&mut self) {
        self.video = !self.video;
        let Some(t) = self.now_playing().cloned() else {
            if let Some(p) = &mut self.player {
                p.set_video(self.video);
            }
            return;
        };
        let at = self.status.time;
        if self.video {
            if let Some(p) = &mut self.player {
                p.set_video(true);
            }
            float::pop_video();
            self.status.loading = true;
            self.fetch(move || Some(Msg::Video(t.id.clone(), ytm::find_video(&t))));
        } else if let Some(p) = &mut self.player {
            // Back to the audio stream, at the same spot.
            p.set_video(false);
            p.load(&t.url(), self.quality.format(), (at > 1.0).then_some(at));
        }
    }

    fn toggle_float_lyrics(&mut self) {
        self.float_lyrics = !self.float_lyrics;
        self.float_last = None;
        float::show(self.float_lyrics);
        self.push_float();
    }

    /// Send the corner overlay the lines around the one playing (only when they change).
    fn push_float(&mut self) {
        if !self.float_lyrics {
            return;
        }
        let Some(t) = self.now_playing().cloned() else {
            float::set(float::LyricFloat {
                line: "Nothing playing".into(),
                ..Default::default()
            });
            return;
        };
        let l = self.lyrics.get(&t.id);
        let i = l.and_then(|l| l.line_at(self.status.time));
        let key = (t.id.clone(), i);
        if self.float_last.as_ref() == Some(&key) && !matches!(l, Some(lyrics::Lyrics::Loading)) {
            return;
        }
        self.float_last = Some(key);
        let song = format!("{} — {}", t.title, t.artist).to_uppercase();
        let f = match l {
            Some(lyrics::Lyrics::Synced(lines)) => {
                let at = |k: Option<usize>| {
                    k.and_then(|k| lines.get(k))
                        .map(|x| x.1.clone())
                        .unwrap_or_default()
                };
                match i {
                    Some(i) => float::LyricFloat {
                        prev: at(i.checked_sub(1)),
                        line: at(Some(i)),
                        next: at(Some(i + 1)),
                        song,
                    },
                    None => float::LyricFloat {
                        prev: String::new(),
                        line: "♪".into(),
                        next: at(Some(0)),
                        song,
                    },
                }
            }
            Some(lyrics::Lyrics::Plain(_)) => float::LyricFloat {
                line: "Lyrics aren't synced for this song".into(),
                song,
                ..Default::default()
            },
            Some(lyrics::Lyrics::Loading) | None => float::LyricFloat {
                line: "Finding lyrics…".into(),
                song,
                ..Default::default()
            },
            Some(lyrics::Lyrics::Missing) => float::LyricFloat {
                line: "No lyrics for this song".into(),
                song,
                ..Default::default()
            },
        };
        float::set(f);
    }

    fn cycle_vocal(&mut self) {
        self.vocal = self.vocal.next();
        self.apply_vocal(false);
    }

    fn set_engine(&mut self, e: separate::Engine) {
        if self.engine == e {
            return;
        }
        self.engine = e;
        self.save_settings();
        if self.vocal != Vocal::Normal {
            self.apply_vocal(false);
        }
    }

    fn next(&mut self) {
        let Some(i) = self.current else { return };
        if i + 1 < self.queue.len() {
            self.play_index(i + 1);
        } else if self.repeat == Repeat::All && !self.queue.is_empty() {
            self.play_index(0);
        } else {
            self.resume_on_radio = true;
            self.autoplay();
        }
    }

    fn finished(&mut self) {
        match (self.repeat, self.current) {
            (Repeat::One, Some(i)) => self.play_index(i),
            _ => self.next(),
        }
    }

    fn prev(&mut self) {
        // Like most players: restart the song unless we're in its first few seconds.
        if self.status.time > 3.0 {
            if let Some(p) = &mut self.player {
                p.seek(0.0, true);
            }
        } else if let Some(i) = self.current {
            self.play_index(i.saturating_sub(1));
        }
    }

    fn toggle(&mut self) {
        if self.current.is_none() {
            if !self.results.is_empty() {
                self.play_from(self.results.clone(), 0);
            }
        } else if let Some(p) = &mut self.player {
            p.toggle_pause();
        }
    }

    fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.shuffle
            && let Some(i) = self.current
        {
            let mut rest = self.queue.split_off(i + 1);
            shuffle(&mut rest);
            self.queue.extend(rest);
        }
    }

    fn cycle_repeat(&mut self) {
        self.repeat = match self.repeat {
            Repeat::Off => Repeat::All,
            Repeat::All => Repeat::One,
            Repeat::One => Repeat::Off,
        };
    }

    fn seek(&mut self, by: f64) {
        if let Some(p) = &mut self.player {
            p.seek(by, false);
        }
    }

    fn seek_to(&mut self, frac: f32) {
        let d = self.status.duration;
        if d > 0.0 {
            self.status.time = d * frac as f64;
            if let Some(p) = &mut self.player {
                p.seek(self.status.time, true);
            }
        }
    }

    /// Switch quality, save it, and reload the current song at the same spot.
    fn set_quality(&mut self, q: Quality) {
        if q == self.quality {
            return;
        }
        self.quality = q;
        self.save_settings();
        let (Some(track), at) = (self.now_playing().cloned(), self.status.time) else {
            return;
        };
        if self.stem_playing.is_some() || self.video {
            return;
        }
        if let Some(p) = &mut self.player {
            p.load(&track.url(), q.format(), (at > 1.0).then_some(at));
        }
    }

    fn save_settings(&self) {
        if let Some(path) = settings_path()
            && let Err(e) = crate::storage::write_atomic(
                &path,
                format!(
                    "quality={}\nart={}\nengine={}\n",
                    self.quality.id(),
                    self.art_style.id(),
                    self.engine.id()
                )
                .as_bytes(),
            )
        {
            eprintln!("encore: couldn't save settings: {e}");
        }
    }

    /// Switch the artwork style and redraw every cover already fetched.
    fn set_art_style(&mut self, style: art::Style) {
        if style == self.art_style {
            return;
        }
        self.art_style = style;
        self.save_settings();
        self.redraw_art();
    }

    /// Redraw every cover already fetched in the current style and accent.
    fn redraw_art(&mut self) {
        let style = self.art_style;
        self.art_gen += 1;
        let (Some(waker), generation) = (self.waker.clone(), self.art_gen) else {
            return;
        };
        let all: Vec<(String, Arc<[u8]>)> = self
            .art_raw
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let inbox = self.art_inbox.clone();
        let accent = theme::current().accent_rgb();
        // A few workers, newest-played covers don't matter: they all land within a second.
        for chunk in all.chunks(all.len().div_ceil(4).max(1)) {
            let (chunk, inbox, waker) = (chunk.to_vec(), inbox.clone(), waker.clone());
            std::thread::spawn(move || {
                for (id, bytes) in chunk {
                    let done = art::process(&bytes, style, accent);
                    inbox.lock().unwrap().push((id, bytes, generation, done));
                    waker();
                }
            });
        }
    }

    fn set_volume(&mut self, v: f32) {
        self.volume = v.clamp(0.0, 100.0);
        if let Some(p) = &mut self.player {
            p.set_volume(self.volume as f64);
        }
    }

    fn now_playing(&self) -> Option<&Track> {
        self.queue.get(self.current?)
    }

    /// Fetch cover art for these tracks on background threads; it lands in `poll`.
    fn want_art(&mut self, tracks: &[Track]) {
        let Some(waker) = self.waker.clone() else {
            return;
        };
        let todo: Vec<(String, String)> = tracks
            .iter()
            .filter(|t| !t.thumb.is_empty() && self.art_requested.insert(t.id.clone()))
            .map(|t| (t.id.clone(), t.big_thumb()))
            .collect();
        if todo.is_empty() {
            return;
        }
        let inbox = self.art_inbox.clone();
        let (style, generation) = (self.art_style, self.art_gen);
        let accent = theme::current().accent_rgb();
        std::thread::spawn(move || {
            for (id, url) in todo {
                if let Some(bytes) = ytm::fetch(&url) {
                    let bytes: Arc<[u8]> = bytes.into();
                    let done = art::process(&bytes, style, accent);
                    inbox.lock().unwrap().push((id, bytes, generation, done));
                    waker();
                }
            }
        });
    }
}

/// Covers kept beyond the ones on screen. Each costs about 1 MB decoded plus its texture, so
/// an unbounded cache grows by hundreds of MB over an evening of searching.
const ART_SPARE: usize = 48;

impl App {
    /// Drop the oldest covers that no list on any page shows, past `ART_SPARE` of them. A
    /// dropped cover is forgotten by `art_requested` too, so it's fetched again if it returns.
    fn trim_art(&mut self) {
        let h = &self.home;
        let shown: HashSet<&str> = h
            .picks
            .iter()
            .chain(&h.hits)
            .chain(&h.chill)
            .chain(&h.feel_good)
            .chain(h.seed.iter())
            .chain(
                h.mixes
                    .iter()
                    .flat_map(|(seed, l)| std::iter::once(seed).chain(l)),
            )
            .chain(&self.results)
            .chain(&self.queue)
            .chain(&self.history)
            .chain(&self.taste.liked)
            .map(|t| t.id.as_str())
            .collect();
        let spare = self
            .art_order
            .iter()
            .filter(|id| !shown.contains(id.as_str()))
            .count();
        if spare <= ART_SPARE {
            return;
        }
        let mut drop: HashSet<String> = HashSet::new();
        for id in &self.art_order {
            if drop.len() == spare - ART_SPARE {
                break;
            }
            if !shown.contains(id.as_str()) {
                drop.insert(id.clone());
            }
        }
        self.art_order.retain(|id| !drop.contains(id));
        for id in &drop {
            self.art.remove(id);
            self.ascii.remove(id);
            self.art_raw.remove(id);
            self.art_requested.remove(id);
        }
    }
}

/// A small xorshift shuffle; good enough for a playlist, no extra crate.
fn shuffle<T>(v: &mut [T]) {
    let mut x = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(7)
        | 1;
    for i in (1..v.len()).rev() {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        v.swap(i, (x % (i as u64 + 1)) as usize);
    }
}

fn poll(s: &mut App, _: f64, tx: &mut Tx) -> bool {
    let mut changed = false;

    // A new Omarchy theme (or glassy switched on or off): repaint, and redraw the covers
    // whose ink is the accent.
    if theme::generation() != s.theme_seen {
        s.theme_seen = theme::generation();
        changed = true;
        if s.art_style == art::Style::Dither {
            s.redraw_art();
        }
    }

    if let Some(answer) = s.search.take() {
        changed = true;
        match answer {
            Ok(tracks) => {
                s.message = tracks.is_empty().then(|| "No songs found".to_string());
                s.want_art(&tracks);
                s.results = tracks;
            }
            Err(e) => s.message = Some(e),
        }
    }

    let msgs: Vec<Msg> = std::mem::take(&mut *s.inbox.lock().unwrap());
    for m in msgs {
        changed = true;
        match m {
            Msg::Picks(seed, list) => {
                s.want_art(&list[..list.len().min(16)]);
                s.home.seed = seed;
                s.home.picks = list;
                s.home.loaded = true;
            }
            Msg::Hits(l) => {
                s.want_art(&l);
                s.home.hits = l;
            }
            Msg::Chill(l) => {
                s.want_art(&l);
                s.home.chill = l;
            }
            Msg::FeelGood(l) => {
                s.want_art(&l);
                s.home.feel_good = l;
            }
            Msg::Mixes(m) => {
                let firsts: Vec<Track> = m
                    .iter()
                    .flat_map(|(seed, l)| {
                        std::iter::once(seed.clone()).chain(l.iter().take(1).cloned())
                    })
                    .collect();
                s.want_art(&firsts);
                s.home.mixes = m;
            }
            Msg::Station(epoch, list) => s.land_station(epoch, list),
            Msg::StemProgress(key, p) => {
                if let Some(v) = s.stem_jobs.get_mut(&key) {
                    *v = p;
                }
            }
            Msg::StemDone(id, engine, r) => s.stems_done(id, engine, r),
            Msg::Lyrics(id, l) => {
                s.lyrics.insert(id, l);
                s.float_last = None;
                s.push_float();
            }
            Msg::Video(song, found) => {
                let here = s.now_playing().is_some_and(|t| t.id == song);
                if s.video && here {
                    let at = s.status.time;
                    let url = format!(
                        "https://music.youtube.com/watch?v={}",
                        found.as_deref().unwrap_or(&song)
                    );
                    if let Some(p) = &mut s.player {
                        p.set_video(true);
                        p.load(&url, VIDEO_FORMAT, (at > 1.0).then_some(at));
                    }
                }
            }
        }
    }

    let landed: Vec<_> = std::mem::take(&mut *s.art_inbox.lock().unwrap());
    let any_art = !landed.is_empty();
    if let Some(cache) = &s.images {
        for (id, bytes, generation, done) in landed {
            if s.art_raw.insert(id.clone(), bytes.clone()).is_none() {
                s.art_order.push_back(id.clone());
            }
            // Drawn in a style we've since left: the redraw for the new one is on its way.
            if generation != s.art_gen {
                continue;
            }
            match done {
                Some(p) => {
                    s.art.insert(id.clone(), cache.rgba(p.w, p.h, p.rgba));
                    s.ascii.insert(id, p.ascii);
                }
                // A format we can't decode: show it as it is.
                None => {
                    s.art.insert(id, cache.load(bytes));
                }
            }
            changed = true;
        }
    }
    if any_art {
        s.trim_art();
    }

    if s.demo {
        return changed;
    }
    if let Some(p) = &s.player {
        let now = p.status.lock().unwrap().clone();
        if now.ended != s.ended_seen {
            s.ended_seen = now.ended;
            s.status = now;
            s.note_outcome(taste::Outcome::Complete);
            s.finished();
            return true;
        }
        // You closed the video window: back to audio, same song, same spot.
        if now.video_closed != s.video_closed_seen {
            s.video_closed_seen = now.video_closed;
            if s.video {
                s.status = now.clone();
                s.toggle_video();
                return true;
            }
        }
        // Follow the lyrics: the line playing scrolls into view.
        if s.page == Page::Lyrics {
            let line = s
                .now_playing()
                .and_then(|t| s.lyrics.get(&t.id))
                .and_then(|l| l.line_at(now.time));
            if line != s.lyric_line {
                s.lyric_line = line;
                if let Some(i) = line {
                    tx.scroll_into_view(format!("lyric-{i}"));
                }
                changed = true;
            }
        }
        let old = &s.status;
        // Only the clock and the progress bar follow the time, and the clock moves once a
        // second: redraw then, not on every quarter-second wake (lyrics redraw above on
        // their own line changes, and the corner overlay is fed below either way).
        if now.time.floor() != old.time.floor()
            || now.duration != old.duration
            || now.paused != old.paused
            || now.loading != old.loading
            || now.error != old.error
            || now.codec != old.codec
            || now.rate != old.rate
            || now.bitrate != old.bitrate
        {
            changed = true;
        }
        s.status = now;
        s.push_float();
    }
    changed
}

fn on_key(s: &mut App, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if s.query.focused {
        if ev.key == Key::Escape {
            tx.blur();
            return true;
        }
        return false;
    }
    match &ev.key {
        Key::Text(t) if t == " " => s.toggle(),
        Key::Text(t) if t == "n" => s.next(),
        Key::Text(t) if t == "p" => s.prev(),
        Key::Text(t) if t == "s" => s.toggle_shuffle(),
        Key::Text(t) if t == "r" => s.cycle_repeat(),
        Key::Text(t) if t == "y" => s.toggle_lyrics(),
        Key::Text(t) if t == "f" => s.toggle_float_lyrics(),
        Key::Text(t) if t == "v" => s.toggle_video(),
        Key::Text(t) if t == "k" => s.cycle_vocal(),
        Key::Text(t) if t == "l" => {
            if let Some(t) = s.now_playing().cloned() {
                s.toggle_like(&t);
            }
        }
        Key::Text(t) if t == "/" => {
            s.page = Page::Search;
            s.query.focused = true;
        }
        Key::Right => s.seek(10.0),
        Key::Left => s.seek(-10.0),
        Key::Up => s.set_volume(s.volume + 5.0),
        Key::Down => s.set_volume(s.volume - 5.0),
        _ => return false,
    }
    true
}

/// Sample data for headless snapshots (`--snap out.png --scene playing`).
fn demo(s: &mut App) {
    let t = |id: &str, title: &str, artist: &str, album: &str, d: &str| Track {
        id: id.into(),
        title: title.into(),
        artist: artist.into(),
        album: album.into(),
        duration: d.into(),
        thumb: String::new(),
    };
    s.results = vec![
        t("a", "One More Time", "Daft Punk", "Discovery", "5:21"),
        t("b", "Digital Love", "Daft Punk", "Discovery", "4:59"),
        t(
            "c",
            "Harder, Better, Faster, Stronger",
            "Daft Punk",
            "Discovery",
            "3:45",
        ),
        t(
            "d",
            "Instant Crush (feat. Julian Casablancas)",
            "Daft Punk",
            "Random Access Memories",
            "5:38",
        ),
        t("e", "Something About Us", "Daft Punk", "Discovery", "3:52"),
        t(
            "f",
            "Get Lucky (feat. Pharrell Williams & Nile Rodgers)",
            "Daft Punk",
            "Random Access Memories",
            "6:09",
        ),
        t("g", "Veridis Quo", "Daft Punk", "Discovery", "5:45"),
        t("h", "Around the World", "Daft Punk", "Homework", "7:09"),
    ];
    s.query = TextEdit::new("daft punk");
    s.searched = "daft punk".into();
    s.queue = s.results.clone();
    s.current = Some(1);
    s.history = s.results[..4].to_vec();
    s.status = mpv::Status {
        time: 104.0,
        duration: 299.0,
        paused: false,
        codec: "opus".into(),
        rate: 48000,
        bitrate: 141_000.0,
        ..Default::default()
    };
    s.demo = true;
}

fn demo_home(s: &mut App) {
    demo(s);
    s.page = Page::Home;
    s.home.seed = s.history.first().cloned();
    s.home.picks = s.results.iter().rev().cloned().collect();
    s.home.hits = s.results.clone();
    s.home.chill = s.results.clone();
    s.home.feel_good = s.results.clone();
}

fn demo_lyrics(s: &mut App) {
    demo(s);
    s.page = Page::Lyrics;
    let n: usize = std::env::var("ENCORE_LYRIC_LINES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(40);
    let lines: Vec<(f64, String)> = (0..n)
        .map(|i| {
            (
                i as f64 * 4.0,
                format!("This is lyric line number {i}, long enough to wrap across the page width"),
            )
        })
        .collect();
    s.lyrics.insert("b".into(), lyrics::Lyrics::Synced(lines));
    s.lyric_line = Some(26);
}

fn demo_settings(s: &mut App) {
    demo(s);
    s.page = Page::Settings;
}

pub(crate) fn run() {
    // Hyprland and the launcher match the window to encore.desktop (and its icon) by this.
    rune::set_app_id("encore");
    let mut app = rune::App::new(AppSpec {
        setup: Some(setup),
        scenes: &[
            ("playing", demo),
            ("home", demo_home),
            ("settings", demo_settings),
            ("lyrics", demo_lyrics),
        ],
        poll: Some(poll),
        on_key: Some(on_key),
        min_size: Some((980.0, 620.0)),
        ..AppSpec::new("Encore", (1240.0, 800.0), init, ui::view)
    })
    // See-through, so the background can follow Omarchy's glassy windows (ui::pal).
    .window_options(rune::WindowOptions {
        transparent: true,
        ..Default::default()
    });
    // Floating lyrics: bottom-right corner, over every app, clicks pass through.
    let overlay = app.overlay(
        rune::OverlaySpec {
            name: "encore-lyrics",
            size: float::SIZE,
            edge: rune::ScreenEdge::Bottom,
            margin: 28.0,
            align: rune::Align::End,
            inset: 28.0,
            interactive: false,
            view: float::view,
        },
        float::LyricFloat::default(),
    );
    overlay.hide();
    float::install(overlay);
    // Debug: `ENCORE_FLOAT=1` shows the corner lyrics at launch.
    if std::env::var_os("ENCORE_FLOAT").is_some() {
        float::show(true);
    }
    // Debug: `ENCORE_FLOAT_CYCLE=1` hides and shows it every half second.
    if std::env::var_os("ENCORE_FLOAT_CYCLE").is_some() {
        std::thread::spawn(|| {
            for i in 0..16 {
                std::thread::sleep(std::time::Duration::from_millis(500));
                float::show(i % 2 == 1);
                eprintln!("encore: float {}", if i % 2 == 1 { "show" } else { "hide" });
            }
        });
    }
    app.run();
}
