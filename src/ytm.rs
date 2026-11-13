//! YouTube Music search through its web client's own JSON endpoint (what ytmusicapi uses).
//! Unofficial: if YouTube changes the response shape, `parse` is the one place to fix.

use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: String,
    pub thumb: String,
}

impl Track {
    /// The cover at 544 px for the big artwork (search gives 120 px; the URL says the size).
    pub fn big_thumb(&self) -> String {
        match self.thumb.find("=w") {
            Some(i) if self.thumb.contains("googleusercontent") => format!("{}=w544-h544-l90-rj", &self.thumb[..i]),
            _ => self.thumb.clone(),
        }
    }

    pub fn url(&self) -> String {
        format!("https://music.youtube.com/watch?v={}", self.id)
    }
}

const ENDPOINT: &str = "https://music.youtube.com/youtubei/v1/search?prettyPrint=false";
/// The "Songs" filter chip.
const SONGS: &str = "EgWKAQIIAWoKEAkQBRAKEAMQBA==";

fn client() -> Value {
    json!({ "client": { "clientName": "WEB_REMIX", "clientVersion": "1.20241106.01.00", "hl": "en" } })
}

fn call(endpoint: &str, mut body: Value) -> Result<Value, String> {
    body["context"] = client();
    ureq::post(endpoint)
        .set("Origin", "https://music.youtube.com")
        .set("User-Agent", "Mozilla/5.0 (X11; Linux) encore")
        .timeout(std::time::Duration::from_secs(10))
        .send_json(body)
        .map_err(|e| format!("couldn't reach YouTube Music: {e}"))?
        .into_json()
        .map_err(|e| format!("bad response: {e}"))
}

pub fn search(query: &str) -> Result<Vec<Track>, String> {
    let resp = call(ENDPOINT, json!({ "query": query, "params": SONGS }))?;
    Ok(parse(&resp))
}

/// Filler uploads that game search ("Top Hits Playlist" by "Todays Top Hits"): not real songs.
pub fn is_filler(t: &Track) -> bool {
    let a = t.artist.to_lowercase();
    let n = t.title.to_lowercase();
    const WORDS: [&str; 8] = ["top hits", "playlist", "top 100", "top 50", "best songs", "hits 20", "mix 20", "compilation"];
    WORDS.iter().any(|w| a.contains(w) || n.contains(w)) || a.contains("music channel")
}

/// Search, with filler dropped (unless that would leave nothing).
pub fn search_clean(query: &str) -> Result<Vec<Track>, String> {
    let all = search(query)?;
    let clean: Vec<Track> = all.iter().filter(|t| !is_filler(t)).cloned().collect();
    Ok(if clean.is_empty() { all } else { clean })
}

/// The official music video for a song, if YouTube Music has one: the "Videos" filter.
pub fn find_video(t: &Track) -> Option<String> {
    let q = format!("{} {}", crate::taste::primary_artist(&t.artist), t.title);
    let resp = call(ENDPOINT, json!({ "query": q, "params": "EgWKAQIQAWoKEAkQChAFEAMQBA==" })).ok()?;
    let want = crate::taste::primary_artist(&t.artist);
    // Score each hit: the artist's own channel and "official" win; karaoke, covers, lyric
    // videos and live cuts lose (search often ranks a karaoke version first).
    let score = |v: &Track| {
        let title = v.title.to_lowercase();
        let mut s = 0i32;
        if crate::taste::primary_artist(&v.artist) == want {
            s += 4;
        }
        if title.contains("official") {
            s += 2;
        }
        for bad in ["karaoke", "cover", "lyric", "live", "reaction", "instrumental", "8d", "slowed", "sped up", "remix"] {
            if title.contains(bad) && !t.title.to_lowercase().contains(bad) {
                s -= 5;
            }
        }
        s
    };
    let best = parse(&resp).into_iter().take(6).max_by_key(|v| score(v))?;
    (score(&best) > 0).then_some(best.id)
}

/// A radio station seeded by one song: what YouTube Music plays "up next" after it
/// (about 50 related songs, the seed first).
pub fn radio(video_id: &str) -> Result<Vec<Track>, String> {
    let resp = call(
        "https://music.youtube.com/youtubei/v1/next?prettyPrint=false",
        json!({ "videoId": video_id, "playlistId": format!("RDAMVM{video_id}"), "isAudioOnly": true }),
    )?;
    let items = &resp["contents"]["singleColumnMusicWatchNextResultsRenderer"]["tabbedRenderer"]
        ["watchNextTabbedResultsRenderer"]["tabs"][0]["tabRenderer"]["content"]["musicQueueRenderer"]["content"]
        ["playlistPanelRenderer"]["contents"];
    let runs = |v: &Value| -> String {
        v["runs"].as_array().into_iter().flatten().filter_map(|r| r["text"].as_str()).collect::<String>()
    };
    let mut out = Vec::new();
    for it in items.as_array().into_iter().flatten() {
        let r = &it["playlistPanelVideoRenderer"];
        let Some(id) = r["videoId"].as_str() else { continue };
        // "Artist • Album • 2001"
        let by = runs(&r["longBylineText"]);
        let parts: Vec<&str> = by.split(" • ").collect();
        out.push(Track {
            id: id.to_string(),
            title: runs(&r["title"]),
            artist: parts.first().unwrap_or(&"").to_string(),
            album: if parts.len() >= 2 && !parts[1].chars().all(|c| c.is_ascii_digit()) { parts[1].to_string() } else { String::new() },
            duration: runs(&r["lengthText"]),
            thumb: r["thumbnail"]["thumbnails"].as_array().and_then(|a| a.last()).and_then(|t| t["url"].as_str()).unwrap_or_default().to_string(),
        });
    }
    if out.is_empty() { Err("no radio for this song".into()) } else { Ok(out) }
}

impl Track {
    pub fn to_json(&self) -> Value {
        json!({ "id": self.id, "title": self.title, "artist": self.artist, "album": self.album, "duration": self.duration, "thumb": self.thumb })
    }
    pub fn from_json(v: &Value) -> Option<Track> {
        let f = |k: &str| v[k].as_str().unwrap_or_default().to_string();
        Some(Track { id: v["id"].as_str()?.to_string(), title: f("title"), artist: f("artist"), album: f("album"), duration: f("duration"), thumb: f("thumb") })
    }
}

fn parse(v: &Value) -> Vec<Track> {
    let sections = &v["contents"]["tabbedSearchResultsRenderer"]["tabs"][0]["tabRenderer"]["content"]
        ["sectionListRenderer"]["contents"];
    let mut out = Vec::new();
    for sec in sections.as_array().into_iter().flatten() {
        for item in sec["musicShelfRenderer"]["contents"].as_array().into_iter().flatten() {
            if let Some(t) = track(&item["musicResponsiveListItemRenderer"]) {
                out.push(t);
            }
        }
    }
    out
}

fn track(r: &Value) -> Option<Track> {
    let id = r["playlistItemData"]["videoId"].as_str()?.to_string();
    let col = |i: usize| -> Vec<String> {
        r["flexColumns"][i]["musicResponsiveListItemFlexColumnRenderer"]["text"]["runs"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|run| run["text"].as_str().map(str::to_string))
            .collect()
    };
    let title = col(0).concat();
    // "Artist • Album • 3:45" (artists can be several runs joined by " & " or ", ").
    let meta = col(1).concat();
    let parts: Vec<&str> = meta.split(" • ").collect();
    let duration = parts.last().filter(|p| p.contains(':')).map(|p| p.to_string()).unwrap_or_default();
    let artist = parts.first().map(|p| p.to_string()).unwrap_or_default();
    let album = if parts.len() >= 3 { parts[parts.len() - 2].to_string() } else { String::new() };
    let thumb = r["thumbnail"]["musicThumbnailRenderer"]["thumbnail"]["thumbnails"]
        .as_array()
        .and_then(|a| a.last())
        .and_then(|t| t["url"].as_str())
        .unwrap_or_default()
        .to_string();
    Some(Track { id, title, artist, album, duration, thumb })
}

/// Fetch cover art bytes (blocking; call off the UI thread).
pub fn fetch(url: &str) -> Option<Vec<u8>> {
    let resp = ureq::get(url).timeout(std::time::Duration::from_secs(10)).call().ok()?;
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut resp.into_reader(), &mut bytes).ok()?;
    Some(bytes)
}
