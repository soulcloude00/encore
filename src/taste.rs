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
        if !time.is_finite() || !duration.is_finite() || time < 0.0 || duration < 0.0 {
            return Outcome::Partial;
        }
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
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn decay(last: u64, now: u64) -> f64 {
    let days = now.saturating_sub(last) as f64 / 86_400.0;
    0.5f64.powf(days / HALF_LIFE_DAYS)
}

impl Taste {
    pub fn load(path: &std::path::Path) -> Taste {
        let Ok(text) = std::fs::read_to_string(path) else {
            return Taste::default();
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else {
            return Taste::default();
        };
        let liked = v["liked"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Track::from_json)
            .collect();
        let mut stats = HashMap::new();
        for s in v["stats"].as_array().into_iter().flatten() {
            let Some(track) = Track::from_json(&s["track"]) else {
                continue;
            };
            let n = |k: &str| s[k].as_u64().unwrap_or(0);
            stats.insert(
                track.id.clone(),
                Stat {
                    track,
                    plays: n("plays") as u32,
                    completes: n("completes") as u32,
                    skips: n("skips") as u32,
                    last: n("last"),
                },
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
        if let Err(e) = crate::storage::write_atomic(
            path,
            json!({ "liked": liked, "stats": stats })
                .to_string()
                .as_bytes(),
        ) {
            eprintln!("encore: couldn't save listening preferences: {e}");
        }
    }

    pub fn record(&mut self, t: &Track, outcome: Outcome) {
        let s = self.stats.entry(t.id.clone()).or_insert_with(|| Stat {
            track: t.clone(),
            plays: 0,
            completes: 0,
            skips: 0,
            last: 0,
        });
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
            Some(s) => {
                liked
                    + (s.completes as f64 * 1.0 - s.skips as f64 * 1.2 + s.plays as f64 * 0.1)
                        * decay(s.last, now)
            }
            None => liked,
        }
    }

    /// Per-artist affinity, summed over their songs.
    pub fn artist_scores(&self, now: u64) -> HashMap<String, f64> {
        let mut out: HashMap<String, f64> = HashMap::new();
        for s in self.stats.values() {
            *out.entry(primary_artist(&s.track.artist)).or_default() +=
                self.song_score(&s.track.id, now);
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
        if n == 0 {
            return Vec::new();
        }
        let now = now();
        let mut cands: Vec<(f64, &Track)> = self
            .liked
            .iter()
            .map(|t| (self.song_score(&t.id, now), t))
            .collect();
        for s in self.stats.values() {
            if !self.is_liked(&s.track.id) {
                cands.push((self.song_score(&s.track.id, now), &s.track));
            }
        }
        cands.retain(|(score, _)| *score > 0.3);
        cands.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
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
        self.stats
            .get(id)
            .is_some_and(|s| s.skips >= 2 && s.completes == 0)
            && !self.is_liked(id)
    }

    /// Merge candidate lists (each in the source's relevance order) into one ranked list:
    /// relevance first, nudged by your artist affinity, with recent plays, dislikes and filler
    /// dropped, and artists spaced out.
    pub fn rank(&self, lists: &[Vec<Track>], avoid: &HashSet<String>, limit: usize) -> Vec<Track> {
        if limit == 0 {
            return Vec::new();
        }
        let now = now();
        let artists = self.artist_scores(now);
        let mut best: HashMap<String, (f64, Track)> = HashMap::new();
        for list in lists {
            let n = list.len().max(1) as f64;
            let mut seen = HashSet::new();
            for (pos, t) in list.iter().enumerate() {
                if !seen.insert(&t.id)
                    || avoid.contains(&t.id)
                    || self.disliked(&t.id)
                    || ytm::is_filler(t)
                {
                    continue;
                }
                // A source votes once per track. Independent sources add relevance;
                // personal affinity is applied once after all votes are counted.
                let relevance = 1.0 - pos as f64 / n;
                let entry = best.entry(t.id.clone()).or_insert((0.0, t.clone()));
                entry.0 += relevance;
            }
        }
        let mut ranked: Vec<(f64, Track)> = best.into_values().collect();
        for (score, track) in &mut ranked {
            let affinity = artists
                .get(&primary_artist(&track.artist))
                .copied()
                .unwrap_or(0.0);
            *score += (affinity * 0.15).clamp(-0.6, 0.35);
        }
        // Hash-map iteration must never determine a listening order.
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
        // Different uploads of the same named recording should not fill a station.
        // Keep version qualifiers (live/remix/etc.) in the title so those remain distinct.
        let mut recordings = HashSet::new();
        let unique = ranked
            .into_iter()
            .map(|(_, t)| t)
            .filter(|t| recordings.insert((primary_artist(&t.artist), normalized_title(&t.title))))
            .collect();
        spread_artists(unique, 3).into_iter().take(limit).collect()
    }
}

/// "The Weeknd & Anitta" → "the weeknd": artist credit for grouping.
pub fn primary_artist(artist: &str) -> String {
    let a = artist.to_lowercase();
    let cut = [" & ", ", ", " feat. ", " feat ", " x ", " ft. "]
        .iter()
        .filter_map(|s| a.find(s))
        .min()
        .unwrap_or(a.len());
    a[..cut].trim().to_string()
}

fn normalized_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Reorder around an immutable, already queued tail. Only new candidates move.
pub fn spread_after(mut pool: Vec<Track>, previous: &[Track], gap: usize) -> Vec<Track> {
    let mut recent: std::collections::VecDeque<String> = previous
        .iter()
        .rev()
        .take(gap)
        .map(|t| primary_artist(&t.artist))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let mut out = Vec::with_capacity(pool.len());
    while !pool.is_empty() {
        let pick = pool
            .iter()
            .position(|t| !recent.contains(&primary_artist(&t.artist)))
            .unwrap_or(0);
        let track = pool.remove(pick);
        recent.push_back(primary_artist(&track.artist));
        while recent.len() > gap {
            recent.pop_front();
        }
        out.push(track);
    }
    out
}

pub fn spread_artists(pool: Vec<Track>, gap: usize) -> Vec<Track> {
    spread_after(pool, &[], gap)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, artist: &str) -> Track {
        Track {
            id: id.into(),
            title: id.into(),
            artist: artist.into(),
            album: String::new(),
            duration: String::new(),
            thumb: String::new(),
        }
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
        let out = spread_artists(
            vec![
                t("1", "A"),
                t("2", "A"),
                t("3", "A"),
                t("4", "B"),
                t("5", "C"),
            ],
            2,
        );
        let artists: Vec<&str> = out.iter().map(|t| t.artist.as_str()).collect();
        assert_eq!(&artists[..3], &["A", "B", "C"]);
    }

    #[test]
    fn rank_drops_recent_and_disliked_and_rewards_overlap() {
        let mut taste = Taste::default();
        let bad = t("bad", "X");
        taste.record(&bad, Outcome::Skip);
        taste.record(&bad, Outcome::Skip);
        let lists = vec![
            vec![t("a", "A"), t("bad", "X"), t("b", "B"), t("c", "C")],
            vec![t("c", "C"), t("d", "D")],
        ];
        let avoid: HashSet<String> = ["a".to_string()].into();
        let out: Vec<String> = taste
            .rank(&lists, &avoid, 10)
            .into_iter()
            .map(|t| t.id)
            .collect();
        assert!(!out.contains(&"a".to_string()) && !out.contains(&"bad".to_string()));
        assert_eq!(
            out[0], "c",
            "a song both radios suggest ranks first: {out:?}"
        );
    }

    #[test]
    fn primary_artist_strips_features() {
        assert_eq!(primary_artist("The Weeknd & Anitta"), "the weeknd");
        assert_eq!(primary_artist("Daft Punk feat. Pharrell"), "daft punk");
    }
    #[test]
    fn duplicate_source_entries_do_not_stuff_the_ranking() {
        let taste = Taste::default();
        let out = taste.rank(
            &[
                vec![t("a", "A"), t("a", "A"), t("a", "A")],
                vec![t("b", "B")],
            ],
            &HashSet::new(),
            10,
        );
        assert_eq!(
            out.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
    #[test]
    fn ties_are_stable_and_zero_limits_are_empty() {
        let taste = Taste::default();
        let lists = [vec![t("b", "B")], vec![t("a", "A")]];
        assert_eq!(taste.rank(&lists, &HashSet::new(), 2)[0].id, "a");
        assert!(taste.rank(&lists, &HashSet::new(), 0).is_empty());
        assert!(taste.seeds(0).is_empty());
    }
    #[test]
    fn alternate_uploads_deduplicate_but_live_versions_remain() {
        let mut duplicate = t("2", "Artist");
        duplicate.title = "Song!".into();
        let mut original = t("1", "Artist");
        original.title = "song".into();
        let mut live = t("3", "Artist");
        live.title = "Song (Live)".into();
        let out = Taste::default().rank(&[vec![original, duplicate, live]], &HashSet::new(), 10);
        assert_eq!(out.len(), 2);
        assert_eq!(out[1].id, "3");
    }
    #[test]
    fn spacing_respects_the_existing_tail() {
        let out = spread_after(vec![t("new-a", "A"), t("new-b", "B")], &[t("old", "A")], 3);
        assert_eq!(out[0].id, "new-b");
    }
    #[test]
    fn invalid_playback_samples_are_not_skip_feedback() {
        assert_eq!(Outcome::judge(f64::NAN, 200.0, false), Outcome::Partial);
        assert_eq!(Outcome::judge(-1.0, 200.0, false), Outcome::Partial);
        assert_eq!(primary_artist("Little Feat"), "little feat");
    }
}
