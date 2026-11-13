//! What you like, learned from what you do: songs you finish, skip and ♥.
//! It scores artists and songs, picks seeds for mixes, and ranks candidates for radio and
//! Quick Picks. Saved as `taste.json` beside the settings.

use crate::ytm::{self, Track};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

/// How a play ended.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Outcome {
    /// Played through (to the end, or past 70%).
    Complete,
    /// Moved on in the first 30 s (and before half way).
    Skip,
    /// Somewhere in between: counts as a play, nothing more.
    Partial,
}

impl Outcome {
    pub fn judge(time: f64, duration: f64, ended: bool) -> Outcome {
        if ended || (duration > 0.0 && time / duration >= 0.7) {
            Outcome::Complete
        } else if time < 30.0 && (duration <= 0.0 || time / duration < 0.5) {
            Outcome::Skip
        } else {
            Outcome::Partial
        }
    }
}

#[derive(Clone)]
pub struct Stat {
    pub track: Track,
    pub plays: u32,
    pub completes: u32,
    pub skips: u32,
    /// Unix seconds of the last play.
    pub last: u64,
}

#[derive(Clone, Default)]
pub struct Taste {
    pub stats: HashMap<String, Stat>,
    /// ♥'d songs, newest first.
    pub liked: Vec<Track>,
}

/// Signals fade with a two-week half-life, so the profile follows what you're into now.
const HALF_LIFE_DAYS: f64 = 14.0;

pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn decay(last: u64, now: u64) -> f64 {
    let days = now.saturating_sub(last) as f64 / 86_400.0;
    0.5f64.powf(days / HALF_LIFE_DAYS)
}

impl Taste {
    pub fn load(path: &std::path::Path) -> Taste {
        let Ok(text) = std::fs::read_to_string(path) else { return Taste::default() };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { return Taste::default() };
        let liked = v["liked"].as_array().into_iter().flatten().filter_map(Track::from_json).collect();
        let mut stats = HashMap::new();
        for s in v["stats"].as_array().into_iter().flatten() {
            let Some(track) = Track::from_json(&s["track"]) else { continue };
            let n = |k: &str| s[k].as_u64().unwrap_or(0);
            stats.insert(
                track.id.clone(),
                Stat { track, plays: n("plays") as u32, completes: n("completes") as u32, skips: n("skips") as u32, last: n("last") },
            );
        }
        Taste { stats, liked }
    }

    pub fn save(&self, path: &std::path::Path) {
        let stats: Vec<Value> = self
            .stats
            .values()
            .map(|s| json!({ "track": s.track.to_json(), "plays": s.plays, "completes": s.completes, "skips": s.skips, "last": s.last }))
            .collect();
        let liked: Vec<Value> = self.liked.iter().map(Track::to_json).collect();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, json!({ "liked": liked, "stats": stats }).to_string());
    }

    pub fn record(&mut self, t: &Track, outcome: Outcome) {
        let s = self.stats.entry(t.id.clone()).or_insert_with(|| Stat { track: t.clone(), plays: 0, completes: 0, skips: 0, last: 0 });
        s.track = t.clone();
        s.plays += 1;
        s.last = now();
        match outcome {
            Outcome::Complete => s.completes += 1,
            Outcome::Skip => s.skips += 1,
            Outcome::Partial => {}
        }
    }

    pub fn is_liked(&self, id: &str) -> bool {
        self.liked.iter().any(|t| t.id == id)
    }

    pub fn toggle_like(&mut self, t: &Track) -> bool {
        if self.is_liked(&t.id) {
            self.liked.retain(|x| x.id != t.id);
            false
        } else {
            self.liked.insert(0, t.clone());
            true
        }
    }

    /// How much you like one song: ♥ counts most, then finishing it, against skipping it,
    /// all fading with time.
    pub fn song_score(&self, id: &str, now: u64) -> f64 {
        let liked = if self.is_liked(id) { 3.0 } else { 0.0 };
        match self.stats.get(id) {
            Some(s) => liked + (s.completes as f64 * 1.0 - s.skips as f64 * 1.2 + s.plays as f64 * 0.1) * decay(s.last, now),
            None => liked,
        }
    }

    /// Per-artist affinity, summed over their songs.
    pub fn artist_scores(&self, now: u64) -> HashMap<String, f64> {
        let mut out: HashMap<String, f64> = HashMap::new();
        for s in self.stats.values() {
            *out.entry(primary_artist(&s.track.artist)).or_default() += self.song_score(&s.track.id, now);
        }
        for t in &self.liked {
            if !self.stats.contains_key(&t.id) {
                *out.entry(primary_artist(&t.artist)).or_default() += 3.0;
            }
        }
        out
    }

    /// Songs to build mixes from: your best-liked songs, at most one per artist.
    pub fn seeds(&self, n: usize) -> Vec<Track> {
        let now = now();
        let mut cands: Vec<(f64, &Track)> = self.liked.iter().map(|t| (self.song_score(&t.id, now), t)).collect();
        for s in self.stats.values() {
            if !self.is_liked(&s.track.id) {
                cands.push((self.song_score(&s.track.id, now), &s.track));
            }
        }
        cands.retain(|(score, _)| *score > 0.3);
        cands.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for (_, t) in cands {
            if seen.insert(primary_artist(&t.artist)) {
                out.push(t.clone());
                if out.len() == n {
                    break;
                }
            }
        }
        out
    }

    /// Songs you've made clear you don't want (skipped, never finished, not ♥).
    pub fn disliked(&self, id: &str) -> bool {
        self.stats.get(id).is_some_and(|s| s.skips >= 2 && s.completes == 0) && !self.is_liked(id)
    }

    /// Merge candidate lists (each in the source's relevance order) into one ranked list:
    /// relevance first, nudged by your artist affinity, with recent plays, dislikes and filler
    /// dropped, and artists spaced out.
    pub fn rank(&self, lists: &[Vec<Track>], avoid: &HashSet<String>, limit: usize) -> Vec<Track> {
        let now = now();
        let artists = self.artist_scores(now);
        let mut best: HashMap<String, (f64, Track)> = HashMap::new();
        for list in lists {
            let n = list.len().max(1) as f64;
            for (pos, t) in list.iter().enumerate() {
                if avoid.contains(&t.id) || self.disliked(&t.id) || ytm::is_filler(t) {
                    continue;
                }
                // Position in the source list is the strongest signal (the source's own
                // similarity ranking); appearing in several lists adds up.
                let relevance = 1.0 - pos as f64 / n;
                let affinity = artists.get(&primary_artist(&t.artist)).copied().unwrap_or(0.0);
                // Known artists help, but softly (and only so much), so mixes still discover.
                let taste = (affinity * 0.15).clamp(-0.6, 0.35);
                let e = best.entry(t.id.clone()).or_insert((0.0, t.clone()));
                e.0 += relevance + taste;
            }
        }
        let mut ranked: Vec<(f64, Track)> = best.into_values().collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        spread_artists(ranked.into_iter().map(|(_, t)| t).collect(), 3)
            .into_iter()
            .take(limit)
            .collect()
    }
}

/// "The Weeknd & Anitta" → "the weeknd": artist credit for grouping.
pub fn primary_artist(artist: &str) -> String {
    let a = artist.to_lowercase();
    let cut = [" & ", ", ", " feat", " x ", " ft."].iter().filter_map(|s| a.find(s)).min().unwrap_or(a.len());
    a[..cut].trim().to_string()
}

/// Reorder so no artist repeats within `gap` songs where it can be helped, keeping the
/// order otherwise.
pub fn spread_artists(mut pool: Vec<Track>, gap: usize) -> Vec<Track> {
    let mut out: Vec<Track> = Vec::with_capacity(pool.len());
    while !pool.is_empty() {
        let recent: Vec<String> = out.iter().rev().take(gap).map(|t| primary_artist(&t.artist)).collect();
        let pick = pool.iter().position(|t| !recent.contains(&primary_artist(&t.artist))).unwrap_or(0);
        out.push(pool.remove(pick));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, artist: &str) -> Track {
        Track { id: id.into(), title: id.into(), artist: artist.into(), album: String::new(), duration: String::new(), thumb: String::new() }
    }

    #[test]
    fn outcomes() {
        assert_eq!(Outcome::judge(10.0, 200.0, false), Outcome::Skip);
        assert_eq!(Outcome::judge(150.0, 200.0, false), Outcome::Complete);
        assert_eq!(Outcome::judge(60.0, 200.0, false), Outcome::Partial);
        assert_eq!(Outcome::judge(5.0, 200.0, true), Outcome::Complete);
    }

    #[test]
    fn spreads_artists_apart() {
        let out = spread_artists(vec![t("1", "A"), t("2", "A"), t("3", "A"), t("4", "B"), t("5", "C")], 2);
        let artists: Vec<&str> = out.iter().map(|t| t.artist.as_str()).collect();
        assert_eq!(&artists[..3], &["A", "B", "C"]);
    }

    #[test]
    fn rank_drops_recent_and_disliked_and_rewards_overlap() {
        let mut taste = Taste::default();
        let bad = t("bad", "X");
        taste.record(&bad, Outcome::Skip);
        taste.record(&bad, Outcome::Skip);
        let lists = vec![vec![t("a", "A"), t("bad", "X"), t("b", "B"), t("c", "C")], vec![t("c", "C"), t("d", "D")]];
        let avoid: HashSet<String> = ["a".to_string()].into();
        let out: Vec<String> = taste.rank(&lists, &avoid, 10).into_iter().map(|t| t.id).collect();
        assert!(!out.contains(&"a".to_string()) && !out.contains(&"bad".to_string()));
        assert_eq!(out[0], "c", "a song both radios suggest ranks first: {out:?}");
    }

    #[test]
    fn primary_artist_strips_features() {
        assert_eq!(primary_artist("The Weeknd & Anitta"), "the weeknd");
        assert_eq!(primary_artist("Daft Punk feat. Pharrell"), "daft punk");
    }
}
