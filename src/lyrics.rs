//! Lyrics from LRCLIB (lrclib.net): free, no key, and usually time-synced.

use crate::ytm::Track;
use serde_json::Value;

#[derive(Clone, Debug)]
pub enum Lyrics {
    Loading,
    /// Timed lines: (seconds, text). Empty text is an instrumental gap.
    Synced(Vec<(f64, String)>),
    Plain(Vec<String>),
    Missing,
}

impl Lyrics {
    /// The line playing at `t`, for synced lyrics.
    pub fn line_at(&self, t: f64) -> Option<usize> {
        let Lyrics::Synced(lines) = self else { return None };
        // Show a line a touch early: reading lags hearing.
        let t = t + 0.25;
        lines.iter().rposition(|(at, _)| *at <= t)
    }
}

fn secs(d: &str) -> Option<u32> {
    let mut total = 0u32;
    for part in d.split(':') {
        total = total * 60 + part.trim().parse::<u32>().ok()?;
    }
    Some(total)
}

fn get(url: &str, query: &[(&str, &str)]) -> Option<Value> {
    let mut req = ureq::get(url).set("User-Agent", "Encore music player (https://github.com/anomalyco/rune)").timeout(std::time::Duration::from_secs(10));
    for (k, v) in query {
        req = req.query(k, v);
    }
    req.call().ok()?.into_json().ok()
}

/// Strip what YouTube adds to titles but lyric sites don't have: "(feat. …)", "(Official …)".
fn clean_title(t: &str) -> String {
    let mut s = t.to_string();
    for open in ['(', '['] {
        if let Some(i) = s.find(open) {
            let lower = s[i..].to_lowercase();
            if ["feat", "ft.", "official", "remaster", "lyric", "audio", "video", "from "].iter().any(|w| lower.contains(w)) {
                s.truncate(i);
            }
        }
    }
    s.trim().to_string()
}

pub fn fetch(t: &Track) -> Lyrics {
    let title = clean_title(&t.title);
    let artist = crate::taste::primary_artist(&t.artist);
    let dur = secs(&t.duration).map(|d| d.to_string()).unwrap_or_default();
    let mut q = vec![("track_name", title.as_str()), ("artist_name", artist.as_str())];
    if !dur.is_empty() {
        q.push(("duration", dur.as_str()));
    }
    if let Some(v) = get("https://lrclib.net/api/get", &q) {
        if let Some(l) = from_record(&v) {
            return l;
        }
    }
    // No exact match: search, preferring a synced result.
    let query = format!("{artist} {title}");
    if let Some(Value::Array(hits)) = get("https://lrclib.net/api/search", &[("q", query.as_str())]) {
        let synced = hits.iter().find(|h| h["syncedLyrics"].as_str().is_some_and(|s| !s.is_empty()));
        if let Some(l) = synced.or(hits.first()).and_then(from_record) {
            return l;
        }
    }
    Lyrics::Missing
}

fn from_record(v: &Value) -> Option<Lyrics> {
    if let Some(lrc) = v["syncedLyrics"].as_str().filter(|s| !s.trim().is_empty()) {
        let lines = parse_lrc(lrc);
        if !lines.is_empty() {
            return Some(Lyrics::Synced(lines));
        }
    }
    let plain = v["plainLyrics"].as_str().filter(|s| !s.trim().is_empty())?;
    Some(Lyrics::Plain(plain.lines().map(str::to_string).collect()))
}

/// "[01:02.50]text" (a line may carry several stamps) → sorted (seconds, text).
pub fn parse_lrc(lrc: &str) -> Vec<(f64, String)> {
    let mut out = Vec::new();
    for line in lrc.lines() {
        let mut rest = line.trim();
        let mut stamps = Vec::new();
        while let Some(body) = rest.strip_prefix('[') {
            let Some(end) = body.find(']') else { break };
            let stamp = &body[..end];
            if let Some((m, s)) = stamp.split_once(':') {
                if let (Ok(m), Ok(s)) = (m.parse::<f64>(), s.parse::<f64>()) {
                    stamps.push(m * 60.0 + s);
                }
            }
            rest = &body[end + 1..];
        }
        for at in stamps {
            out.push((at, rest.trim().to_string()));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lrc_and_finds_lines() {
        let l = parse_lrc("[00:01.00]One\n[00:05.50]Two\n[00:03.00][00:09.00]Chorus\n");
        assert_eq!(l.iter().map(|x| x.1.as_str()).collect::<Vec<_>>(), ["One", "Chorus", "Two", "Chorus"]);
        let lyr = Lyrics::Synced(l);
        assert_eq!(lyr.line_at(0.2), None);
        assert_eq!(lyr.line_at(4.0), Some(1));
        assert_eq!(lyr.line_at(100.0), Some(3));
    }

    #[test]
    fn cleans_titles() {
        assert_eq!(clean_title("Starboy (feat. Daft Punk)"), "Starboy");
        assert_eq!(clean_title("Blinding Lights (Official Audio)"), "Blinding Lights");
        assert_eq!(clean_title("Love (Remix)"), "Love (Remix)");
    }
}
