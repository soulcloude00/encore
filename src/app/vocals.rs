//! AI vocals in the player: Vocals and Karaoke play real separated stems. Until a song's
//! stems are ready, the instant stereo filter stands in, so the switch always does something
//! at once; when the stems land, playback moves onto them at the same spot.

use crate::separate::{self, Engine, Stems};
use crate::{App, Msg, Vocal};
use std::sync::Mutex;

/// One separation at a time: they each use every core.
static RUNNING: Mutex<()> = Mutex::new(());

impl App {
    /// Make what's playing match the vocal mode: original stream, a stem, or the stand-in
    /// filter while a stem is being made. `fresh`: a song is starting and nothing is loaded.
    pub(crate) fn apply_vocal(&mut self, fresh: bool) {
        let Some(t) = self.now_playing().cloned() else {
            if let Some(p) = &mut self.player {
                p.set_audio_filter(self.vocal.filter());
            }
            return;
        };
        let at = if fresh { 0.0 } else { self.status.time };
        let start = (at > 1.0).then_some(at);
        // Video plays the music video's own audio: only the filter applies there.
        if self.video {
            if let Some(p) = &mut self.player {
                p.set_audio_filter(self.vocal.filter());
            }
            return;
        }
        if self.vocal == Vocal::Normal {
            if let Some(p) = &mut self.player {
                p.set_audio_filter("");
                if self.stem_playing.take().is_some() || fresh {
                    p.load(&t.url(), self.quality.format(), start);
                }
            }
            return;
        }
        match separate::cached(&t.id, Engine::Fast) {
            Some(stems) => self.play_stem(&t.id, &stems, start),
            None => {
                if let Some(p) = &mut self.player {
                    p.set_audio_filter(self.vocal.filter());
                    if self.stem_playing.take().is_some() || fresh {
                        p.load(&t.url(), self.quality.format(), start);
                    }
                }
                if self.engine == Engine::Fast || separate::fast_models_available() {
                    self.queue_stems(&t.id, Engine::Fast);
                }
            }
        }
        // HQ engine: once the fast stems exist, make the best ones too.
        if self.engine == Engine::Hq && separate::cached(&t.id, Engine::Hq).is_none() {
            self.queue_stems(&t.id, Engine::Hq);
        }
    }

    fn play_stem(&mut self, id: &str, stems: &Stems, start: Option<f64>) {
        let path = if self.vocal == Vocal::Karaoke {
            &stems.instrumental
        } else {
            &stems.vocals
        };
        let path = path.display().to_string();
        if let Some(p) = &mut self.player {
            p.set_audio_filter("");
            p.load(&path, "", start);
        }
        self.stem_playing = Some(id.to_string());
    }

    /// Separate a song in the background (once; one at a time).
    pub(crate) fn queue_stems(&mut self, id: &str, engine: Engine) {
        let key = format!("{id}.{}", engine.id());
        // `cached(id, Fast)` is satisfied by either engine's stems; `cached(id, Hq)` only by HQ.
        if self.stem_jobs.contains_key(&key) || separate::cached(id, engine).is_some() {
            return;
        }
        self.stem_jobs.insert(key.clone(), 0.0);
        let id = id.to_string();
        let url = format!("https://music.youtube.com/watch?v={id}");
        let ytdlp = crate::mpv::ytdl_path();
        let (inbox, waker) = (self.inbox.clone(), self.waker.clone());
        std::thread::spawn(move || {
            let _one = RUNNING.lock();
            let send = |m: Msg| {
                inbox.lock().unwrap().push(m);
                if let Some(w) = &waker {
                    w();
                }
            };
            let last = std::cell::Cell::new(-1i32);
            let r = separate::separate(&id, &url, &ytdlp, engine, &|p| {
                // Report in 5% steps, not every chunk.
                let step = (p * 20.0) as i32;
                if step != last.get() {
                    last.set(step);
                    send(Msg::StemProgress(key.clone(), p));
                }
            });
            send(Msg::StemDone(id, engine, r));
        });
    }

    /// Stems landed: move onto them if they're for what's playing, and get the next song
    /// ready while the mode is on.
    pub(crate) fn stems_done(&mut self, id: String, engine: Engine, r: Result<Stems, String>) {
        self.stem_jobs.remove(&format!("{id}.{}", engine.id()));
        match r {
            Err(e) => self.stem_error = Some(e),
            Ok(stems) => {
                self.stem_error = None;
                let here = self.now_playing().is_some_and(|t| t.id == id);
                if here && self.vocal != Vocal::Normal && !self.video {
                    let at = self.status.time;
                    self.play_stem(&id, &stems, (at > 1.0).then_some(at));
                }
            }
        }
        if self.vocal != Vocal::Normal
            && let Some(next) = self.current.and_then(|i| self.queue.get(i + 1)).cloned()
        {
            let engine = if self.engine == Engine::Hq && !separate::fast_models_available() {
                Engine::Hq
            } else {
                Engine::Fast
            };
            self.queue_stems(&next.id, engine);
        }
    }

    /// "45%" while the playing song separates, for the vocal button.
    pub(crate) fn stem_progress(&self) -> Option<f32> {
        let t = self.now_playing()?;
        self.stem_jobs.get(&format!("{}.fast", t.id)).copied()
    }
}
