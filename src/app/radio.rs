//! Song radio, like Spotify's: start from any song and it plays on forever, drifting with
//! what you finish and steering away from what you skip.

use crate::taste::{self, Outcome};
use crate::ytm::{self, Track};
use crate::{App, Msg};
use std::collections::{HashMap, HashSet};

pub struct Station {
    pub seed: Track,
    /// Songs you played through on this station; refills alternate between the seed and the
    /// newest of these, so the station follows where your listening goes.
    pub good: Vec<Track>,
    artist_skips: HashMap<String, u32>,
    /// Artists skipped twice on this station: gone for its rest.
    pub banned: HashSet<String>,
    refills: u32,
    pending: bool,
    failures: u8,
    epoch: u64,
}

/// Refill when this few songs are left after the one playing.
const LOW_WATER: usize = 5;

impl App {
    /// Song radio from `seed`: plays it, then songs like it, without end.
    pub(crate) fn start_radio(&mut self, seed: Track) {
        self.start_station(seed, Vec::new());
    }

    /// Start a station whose first songs are already known (a Daily Mix).
    pub(crate) fn start_station(&mut self, seed: Track, known: Vec<Track>) {
        self.note_outcome(Outcome::judge(
            self.status.time,
            self.status.duration,
            false,
        ));
        self.current = None;
        self.station_epoch += 1;
        self.station = Some(Station {
            seed: seed.clone(),
            good: Vec::new(),
            artist_skips: HashMap::new(),
            banned: HashSet::new(),
            refills: 0,
            pending: false,
            failures: 0,
            epoch: self.station_epoch,
        });
        let mut queue = vec![seed.clone()];
        let mut seen: HashSet<String> = [seed.id.clone()].into();
        queue.extend(known.into_iter().filter(|t| seen.insert(t.id.clone())));
        self.resume_on_radio = false;
        self.queue = queue;
        self.play_index(0);
    }

    /// The queue (not a station) ran out: carry on as a station from its last song, the way
    /// Spotify's autoplay does.
    pub(crate) fn autoplay(&mut self) {
        if self.station.is_some() {
            self.refill_station();
            return;
        }
        let Some(last) = self.queue.last().cloned() else {
            return;
        };
        self.station_epoch += 1;
        self.station = Some(Station {
            seed: last,
            good: Vec::new(),
            artist_skips: HashMap::new(),
            banned: HashSet::new(),
            refills: 0,
            pending: false,
            failures: 0,
            epoch: self.station_epoch,
        });
        self.refill_station();
    }

    /// Fetch more songs for the station, if it's running low and not already fetching.
    pub(crate) fn refill_station(&mut self) {
        let Some(st) = &mut self.station else { return };
        if st.pending || st.failures >= 2 {
            return;
        }
        st.pending = true;
        // Alternate: the seed keeps the station on theme; your latest finished song lets it
        // drift the way you're listening.
        let from = if st.refills % 2 == 1 {
            st.good.last().cloned().unwrap_or_else(|| st.seed.clone())
        } else {
            st.seed.clone()
        };
        st.refills += 1;
        let epoch = st.epoch;
        self.fetch(move || Some(Msg::Station(epoch, ytm::radio(&from.id))));
    }

    pub(crate) fn station_low(&self, i: usize) -> bool {
        self.station.is_some() && self.queue.len().saturating_sub(i + 1) <= LOW_WATER
    }

    /// New songs for the station: rank them by your taste, drop what it's already played or
    /// you've heard lately, keep banned artists out, and space artists from the queue's tail.
    pub(crate) fn land_station(&mut self, epoch: u64, result: Result<Vec<Track>, String>) {
        let Some(st) = &mut self.station else { return };
        if st.epoch != epoch {
            return;
        }
        st.pending = false;
        let list = match result {
            Ok(list) => list,
            Err(e) => {
                self.station_failed(format!("Radio request failed: {e}"));
                return;
            }
        };
        let banned = st.banned.clone();
        let list = list
            .into_iter()
            .filter(|t| !banned.contains(&taste::primary_artist(&t.artist)))
            .collect();
        let mut avoid: HashSet<String> = self.queue.iter().map(|t| t.id.clone()).collect();
        avoid.extend(self.history.iter().take(40).map(|t| t.id.clone()));
        let fresh = self.taste.rank(&[list], &avoid, 25);
        if fresh.is_empty() {
            self.station_failed(
                "Radio found no new songs. Start another station to continue.".into(),
            );
            return;
        }
        if let Some(st) = &mut self.station {
            st.failures = 0;
        }
        let tail = self.queue.len().saturating_sub(3);
        let more = taste::spread_after(fresh, &self.queue[tail..], 3);
        self.want_art(&more[..more.len().min(8)]);
        self.queue.extend(more);
        if std::mem::take(&mut self.resume_on_radio) {
            self.next();
        }
    }

    /// Always clear pending on failure. Retry once, then stop automatic requests until
    /// a new station is chosen, preventing request loops on exhausted or broken sources.
    fn station_failed(&mut self, message: String) {
        let Some(st) = &mut self.station else { return };
        st.pending = false;
        st.failures = st.failures.saturating_add(1);
        if st.failures < 2 {
            self.refill_station();
        } else {
            self.resume_on_radio = false;
            self.message = Some(message);
        }
    }

    /// Learn from how the current song ended (once per play).
    pub(crate) fn note_outcome(&mut self, outcome: Outcome) {
        if self.current_recorded {
            return;
        }
        let Some(track) = self.now_playing().cloned() else {
            return;
        };
        self.current_recorded = true;
        // A song that never started (failed, or skipped while loading) teaches nothing.
        if self.status.duration <= 0.0 && outcome != Outcome::Complete {
            return;
        }
        self.taste.record(&track, outcome);
        self.save_taste();
        let Some(st) = &mut self.station else { return };
        match outcome {
            Outcome::Complete => st.good.push(track),
            Outcome::Skip => {
                let artist = taste::primary_artist(&track.artist);
                let n = st.artist_skips.entry(artist.clone()).or_default();
                *n += 1;
                if *n >= 2 && artist != taste::primary_artist(&st.seed.artist) {
                    st.banned.insert(artist.clone());
                    // Take their songs out of what's still to come.
                    if let Some(i) = self.current {
                        let (head, rest) = self.queue.split_at(i + 1);
                        let rest: Vec<Track> = rest
                            .iter()
                            .filter(|t| taste::primary_artist(&t.artist) != artist)
                            .cloned()
                            .collect();
                        self.queue = head.iter().cloned().chain(rest).collect();
                    }
                }
            }
            Outcome::Partial => {}
        }
    }

    pub(crate) fn toggle_like(&mut self, t: &Track) {
        self.taste.toggle_like(t);
        self.save_taste();
    }

    pub(crate) fn stop_radio(&mut self) {
        self.station = None;
        self.station_epoch += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str, artist: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            artist: artist.into(),
            album: String::new(),
            duration: String::new(),
            thumb: String::new(),
        }
    }

    fn station() -> App {
        let mut app = super::super::init();
        app.taste = taste::Taste::default();
        app.history.clear();
        app.queue = vec![track("seed", "A")];
        app.station = Some(Station {
            seed: app.queue[0].clone(),
            good: vec![],
            artist_skips: HashMap::new(),
            banned: HashSet::new(),
            refills: 0,
            pending: true,
            failures: 1,
            epoch: 7,
        });
        app
    }

    #[test]
    fn stale_deliveries_cannot_change_a_new_station() {
        let mut app = station();
        app.land_station(6, Ok(vec![track("stale", "B")]));
        assert_eq!(app.queue.len(), 1);
        assert!(app.station.as_ref().unwrap().pending);
    }

    #[test]
    fn failed_station_clears_pending_and_stops_after_budget() {
        let mut app = station();
        app.resume_on_radio = true;
        app.land_station(7, Err("offline".into()));
        let station = app.station.as_ref().unwrap();
        assert!(!station.pending);
        assert_eq!(station.failures, 2);
        assert!(!app.resume_on_radio);
        assert!(app.message.as_ref().unwrap().contains("offline"));
        app.refill_station();
        assert!(!app.station.as_ref().unwrap().pending);
    }

    #[test]
    fn empty_refills_stop_and_success_resets_failure_budget() {
        let mut app = station();
        app.land_station(7, Ok(vec![track("seed", "A")]));
        assert_eq!(app.station.as_ref().unwrap().failures, 2);
        let mut app = station();
        app.land_station(7, Ok(vec![track("a", "A"), track("b", "B")]));
        assert_eq!(app.station.as_ref().unwrap().failures, 0);
        assert_eq!(app.queue[0].id, "seed");
        assert_eq!(app.queue[1].id, "b");
    }
}
