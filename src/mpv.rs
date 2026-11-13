//! Playback through an `mpv` child process, driven over its JSON IPC socket.
//! mpv resolves YouTube URLs itself (via yt-dlp), so we just hand it the watch URL.

use serde_json::{Value, json};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub time: f64,
    pub duration: f64,
    pub paused: bool,
    pub loading: bool,
    /// Bumped each time a file finishes on its own (not when replaced or stopped).
    pub ended: u64,
    pub error: Option<String>,
    /// What's actually streaming: codec ("opus"), sample rate (Hz) and bitrate (bits/s).
    pub codec: String,
    pub rate: u32,
    pub bitrate: f64,
    /// Bumped when you close the video window (Encore turns video off and plays on).
    pub video_closed: u64,
}

pub struct Mpv {
    child: Child,
    sock: UnixStream,
    socket_path: PathBuf,
    pub status: Arc<Mutex<Status>>,
}

impl Mpv {
    pub fn spawn(waker: rune::Waker) -> Result<Mpv, String> {
        let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        let socket_path = dir.join(format!("encore-{}.sock", std::process::id()));
        let _ = std::fs::remove_file(&socket_path);
        let mut cmd = Command::new("mpv");
        cmd
            .args([
                "--idle=yes",
                "--no-video",
                "--no-terminal",
                "--force-window=no",
                "--ytdl-format=bestaudio/best",
                // Keep the signal untouched: no ReplayGain, no volume boost past 100%.
                "--replaygain=no",
                "--volume-max=100",
                "--gapless-audio=weak",
                // A whole song is a few MB, so 32 MiB ahead still buffers it all; keep only a
                // little behind for seeking back (mpv's default back buffer is 150 MiB).
                "--cache=yes",
                "--demuxer-max-bytes=32MiB",
                "--demuxer-max-back-bytes=8MiB",
                // Video mode decodes on the GPU where it can, not on the CPU.
                "--hwdec=auto-safe",
                // Encore drives mpv over IPC: skip the built-in Lua scripts it never uses (each
                // runs its own interpreter thread). The OSC stays for the video window's seek bar.
                "--load-auto-profiles=no",
                "--load-commands=no",
                "--load-console=no",
                "--load-context-menu=no",
                "--load-positioning=no",
                "--load-select=no",
                "--load-stats-overlay=no",
                "--load-scripts=no",
                &format!("--script-opts=ytdl_hook-ytdl_path={}", ytdl_path()),
                &format!("--input-ipc-server={}", socket_path.display()),
                // The video window: its own title, and closing it (or pressing q in it) hides
                // video instead of quitting the player.
                "--title=Encore — Video",
                &format!("--input-conf={}", input_conf().display()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // If Encore dies or exits without dropping us, take mpv down with it.
        // SAFETY: prctl is async-signal-safe and touches only this child.
        unsafe {
            cmd.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }
        let child = cmd
            .spawn()
            .map_err(|e| format!("couldn't start mpv: {e}"))?;

        let mut sock = None;
        for _ in 0..100 {
            if let Ok(s) = UnixStream::connect(&socket_path) {
                sock = Some(s);
                break;
            }
            std::thread::sleep(Duration::from_millis(30));
        }
        let sock = sock.ok_or("mpv didn't open its IPC socket")?;
        let status = Arc::new(Mutex::new(Status { paused: true, ..Default::default() }));

        let reader = BufReader::new(sock.try_clone().map_err(|e| e.to_string())?);
        let st = status.clone();
        std::thread::spawn(move || {
            for line in reader.lines() {
                let Ok(line) = line else { break };
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if handle(&st, &v) {
                    waker();
                }
            }
        });

        let mut mpv = Mpv { child, sock, socket_path, status };
        for (i, prop) in ["time-pos", "duration", "pause", "idle-active", "audio-codec-name", "audio-params/samplerate", "audio-bitrate"].iter().enumerate() {
            mpv.send(json!(["observe_property", i + 1, prop]));
        }
        Ok(mpv)
    }

    fn send(&mut self, cmd: Value) {
        let line = json!({ "command": cmd }).to_string() + "\n";
        let _ = self.sock.write_all(line.as_bytes());
    }

    /// Play `url`, asking yt-dlp for `format` (a yt-dlp format selector).
    pub fn load(&mut self, url: &str, format: &str, start: Option<f64>) {
        if let Ok(mut s) = self.status.lock() {
            s.loading = true;
            s.time = 0.0;
            s.duration = 0.0;
            s.error = None;
            s.codec.clear();
            s.rate = 0;
            s.bitrate = 0.0;
        }
        self.send(json!(["set_property", "ytdl-format", format]));
        match start {
            // Per-file option (mpv ≥ 0.38): only this file starts there.
            Some(t) => self.send(json!(["loadfile", url, "replace", -1, format!("start={t:.1}")])),
            None => self.send(json!(["loadfile", url, "replace"])),
        }
        self.send(json!(["set_property", "pause", false]));
    }

    pub fn toggle_pause(&mut self) {
        self.send(json!(["cycle", "pause"]));
    }

    /// Show video in mpv's own window, or go back to audio only.
    pub fn set_video(&mut self, on: bool) {
        self.send(json!(["set_property", "force-window", if on { "yes" } else { "no" }]));
        self.send(json!(["set_property", "vid", if on { "auto" } else { "no" }]));
    }

    /// An ffmpeg filter chain for the audio ("" for none).
    pub fn set_audio_filter(&mut self, af: &str) {
        self.send(json!(["set_property", "af", af]));
    }

    pub fn set_volume(&mut self, v: f64) {
        self.send(json!(["set_property", "volume", v]));
    }

    pub fn seek(&mut self, secs: f64, absolute: bool) {
        self.send(json!(["seek", secs, if absolute { "absolute" } else { "relative" }]));
    }
}

/// Key bindings for the video window: closing it tells Encore instead of quitting mpv.
fn input_conf() -> PathBuf {
    let path = std::env::temp_dir().join(format!("encore-input-{}.conf", std::process::id()));
    let _ = std::fs::write(&path, "CLOSE_WIN script-message encore-video-closed\nq script-message encore-video-closed\nESC script-message encore-video-closed\n");
    path
}

/// The yt-dlp mpv should use. YouTube breaks old versions often, so prefer one installed per
/// user (`uv tool install yt-dlp`, `pipx install yt-dlp`), which is easy to keep current,
/// over the system package; `ENCORE_YTDLP` overrides both.
pub fn ytdl_path() -> String {
    if let Ok(p) = std::env::var("ENCORE_YTDLP") {
        return p;
    }
    if let Some(home) = std::env::var_os("HOME") {
        let local = PathBuf::from(home).join(".local/bin/yt-dlp");
        if local.exists() {
            return local.display().to_string();
        }
    }
    "yt-dlp".into()
}

/// Apply one IPC message to the status; true when something visible changed.
fn handle(st: &Mutex<Status>, v: &Value) -> bool {
    let Ok(mut s) = st.lock() else { return false };
    match v["event"].as_str() {
        Some("property-change") => {
            let data = &v["data"];
            match v["name"].as_str() {
                Some("time-pos") => {
                    let t = data.as_f64().unwrap_or(0.0);
                    if t > 0.0 {
                        s.loading = false;
                    }
                    // Redraw four times a second: enough for the bar and for lyrics to keep time.
                    let changed = (t * 4.0).floor() != (s.time * 4.0).floor();
                    s.time = t;
                    changed
                }
                Some("duration") => {
                    s.duration = data.as_f64().unwrap_or(0.0);
                    true
                }
                Some("audio-codec-name") => {
                    s.codec = data.as_str().unwrap_or_default().to_string();
                    true
                }
                Some("audio-params/samplerate") => {
                    s.rate = data.as_u64().unwrap_or(0) as u32;
                    true
                }
                Some("audio-bitrate") => {
                    // VBR wobbles every packet; only redraw on a real change.
                    let b = data.as_f64().unwrap_or(0.0);
                    let changed = (b - s.bitrate).abs() > 12_000.0;
                    if changed || s.bitrate == 0.0 {
                        s.bitrate = b;
                    }
                    changed
                }
                Some("pause") => {
                    s.paused = data.as_bool().unwrap_or(false);
                    true
                }
                _ => false,
            }
        }
        Some("client-message") if v["args"][0].as_str() == Some("encore-video-closed") => {
            s.video_closed += 1;
            true
        }
        Some("file-loaded") => {
            s.loading = false;
            true
        }
        Some("end-file") => match v["reason"].as_str() {
            Some("eof") => {
                s.ended += 1;
                true
            }
            Some("error") => {
                s.loading = false;
                s.error = Some(format!(
                    "couldn't play this track ({})",
                    v["file_error"].as_str().unwrap_or("unknown error")
                ));
                true
            }
            _ => false,
        },
        _ => false,
    }
}

impl Drop for Mpv {
    fn drop(&mut self) {
        self.send(json!(["quit"]));
        let _ = self.child.wait();
        let _ = std::fs::remove_file(&self.socket_path);
    }
}
