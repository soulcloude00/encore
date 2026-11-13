//! True vocal separation: UVR's MDX-Net "Kim_Vocal_2" model run through ONNX Runtime, with
//! the short-time Fourier transform done here in Rust. One pass gives the vocals; the
//! instrumental is the song minus the vocals. Mirrors UVR's MDX pipeline: 7680-point STFT,
//! hop 1024, the model sees the lowest 3072 bins × 256 frames, chunks overlap by half an FFT
//! at each end and the overlap is trimmed.

use ort::session::{Session, builder::GraphOptimizationLevel};
use ort::value::Tensor;
use realfft::RealFftPlanner;
use realfft::num_complex::Complex32;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const N_FFT: usize = 7680;
const HOP: usize = 1024;
const DIM_F: usize = 3072;
const DIM_T: usize = 256;
const N_BINS: usize = N_FFT / 2 + 1;
/// Samples per chunk the model sees: 256 frames at hop 1024.
const CHUNK: usize = HOP * (DIM_T - 1);
const TRIM: usize = N_FFT / 2;
/// New samples per chunk once both trimmed ends are dropped.
const GEN: usize = CHUNK - 2 * TRIM;
const RATE: u32 = 44_100;
/// UVR's level correction for this model.
const COMPENSATE: f32 = 1.009;

pub const MODEL_URL: &str =
    "https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx";

pub fn model_path() -> PathBuf {
    crate::paths::models().join("Kim_Vocal_2.onnx")
}

pub fn cache_dir() -> PathBuf {
    crate::paths::cache().join("stems")
}

/// Which separator: Spleeter (small, int8, seconds per song) or Kim_Vocal_2 (big, best).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Engine {
    Fast,
    Hq,
}

impl Engine {
    pub fn id(self) -> &'static str {
        match self {
            Engine::Fast => "fast",
            Engine::Hq => "hq",
        }
    }
    pub fn from_id(s: &str) -> Engine {
        if s == "hq" { Engine::Hq } else { Engine::Fast }
    }
}

pub fn fast_models_available() -> bool {
    ["vocals.int8.onnx", "accompaniment.int8.onnx"]
        .iter()
        .all(|name| models_dir().join("spleeter-int8").join(name).is_file())
}

fn models_dir() -> PathBuf {
    model_path().parent().unwrap().to_path_buf()
}

/// The two stems for a song, once separated.
#[derive(Clone, Debug)]
pub struct Stems {
    pub vocals: PathBuf,
    pub instrumental: PathBuf,
}

fn stem_paths(id: &str, e: Engine) -> Stems {
    let d = cache_dir();
    Stems {
        vocals: d.join(format!("{id}.{}.vocals.wav", e.id())),
        instrumental: d.join(format!("{id}.{}.inst.wav", e.id())),
    }
}

/// Stems already on disk: HQ ones if there are, else fast ones (unless HQ is required).
pub fn cached(id: &str, at_least: Engine) -> Option<Stems> {
    if !crate::paths::valid_media_id(id) {
        return None;
    }
    let ok = |s: Stems| (s.vocals.exists() && s.instrumental.exists()).then_some(s);
    ok(stem_paths(id, Engine::Hq)).or_else(|| {
        if at_least == Engine::Fast {
            ok(stem_paths(id, Engine::Fast))
        } else {
            None
        }
    })
}

/// The loaded model, shared by separations close together: loading it takes about a second.
/// `release_later` unloads it once separating stops, since it holds a few hundred MB.
static SESSION: Mutex<Option<Arc<Mutex<Session>>>> = Mutex::new(None);

/// How long a loaded model waits for the next separation before it's unloaded.
const IDLE: Duration = Duration::from_secs(90);
static LAST_USE: Mutex<Option<Instant>> = Mutex::new(None);

/// Unload the models if nothing separates in the next `IDLE`. Running separations keep
/// their own handle, so this never pulls a model out from under one.
fn release_later() {
    *LAST_USE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    std::thread::spawn(|| {
        std::thread::sleep(IDLE);
        let idle = LAST_USE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some_and(|t| t.elapsed() >= IDLE);
        if idle {
            SESSION.lock().unwrap_or_else(|e| e.into_inner()).take();
            SPLEETER.lock().unwrap_or_else(|e| e.into_inner()).take();
        }
    });
}

fn session() -> Result<Arc<Mutex<Session>>, String> {
    let mut slot = SESSION.lock().map_err(|_| "model lock poisoned")?;
    if let Some(s) = &*slot {
        return Ok(s.clone());
    }
    let path = model_path();
    if !path.exists() {
        download_model(&path)?;
    }
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let s = Session::builder()
        .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
        .and_then(|b| b.with_intra_threads(threads))
        .and_then(|b| b.commit_from_file(&path))
        .map_err(|e| format!("couldn't load the vocal model: {e}"))?;
    let s = Arc::new(Mutex::new(s));
    *slot = Some(s.clone());
    Ok(s)
}

fn download_model(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    let resp = ureq::get(MODEL_URL)
        .call()
        .map_err(|e| format!("couldn't download the vocal model: {e}"))?;
    let tmp = path.with_extension("part");
    let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
    std::io::copy(&mut resp.into_reader(), &mut f).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Decode a song to 44.1 kHz stereo floats, interleaved. yt-dlp fetches the audio (it knows
/// the headers YouTube wants), ffmpeg decodes it.
fn decode(page_url: &str, ytdlp: &str) -> Result<Vec<f32>, String> {
    std::fs::create_dir_all(cache_dir()).map_err(|e| e.to_string())?;
    let workspace = tempfile::Builder::new()
        .prefix("decode-")
        .tempdir_in(cache_dir())
        .map_err(|e| e.to_string())?;
    let stem = workspace.path().join("audio");
    let out = std::process::Command::new(ytdlp)
        .args([
            "--no-warnings",
            "-q",
            "--no-playlist",
            "-f",
            "251/bestaudio/best",
            "--force-overwrites",
            "-o",
        ])
        .arg(format!("{}.%(ext)s", stem.display()))
        .arg("--print")
        .arg("after_move:filepath")
        .arg(page_url)
        .output()
        .map_err(|e| format!("couldn't run yt-dlp: {e}"))?;
    let file = String::from_utf8_lossy(&out.stdout)
        .lines()
        .last()
        .unwrap_or_default()
        .trim()
        .to_string();
    if !out.status.success() || file.is_empty() || !Path::new(&file).exists() {
        return Err(format!(
            "yt-dlp couldn't fetch the audio: {}",
            String::from_utf8_lossy(&out.stderr)
                .lines()
                .last()
                .unwrap_or("")
        ));
    }
    let mut child = std::process::Command::new("ffmpeg")
        .args([
            "-nostdin",
            "-loglevel",
            "error",
            "-i",
            &file,
            "-f",
            "f32le",
            "-ac",
            "2",
            "-ar",
            "44100",
            "pipe:1",
        ])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("couldn't run ffmpeg: {e}"))?;
    let mut bytes = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err(format!("ffmpeg failed: {status}"));
    }
    if bytes.len() < 8 || bytes.len() % 8 != 0 {
        return Err("ffmpeg decoded nothing".into());
    }
    Ok(bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect())
}

/// Periodic Hann, as torch.hann_window(n) (the window UVR's STFT uses).
fn hann() -> Vec<f32> {
    (0..N_FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos())
        .collect()
}

struct Fft {
    fwd: std::sync::Arc<dyn realfft::RealToComplex<f32>>,
    inv: std::sync::Arc<dyn realfft::ComplexToReal<f32>>,
    win: Vec<f32>,
}

impl Fft {
    fn new() -> Fft {
        let mut p = RealFftPlanner::<f32>::new();
        Fft {
            fwd: p.plan_fft_forward(N_FFT),
            inv: p.plan_fft_inverse(N_FFT),
            win: hann(),
        }
    }

    /// One channel's chunk (CHUNK samples) → the model's input planes (re, im), each
    /// DIM_F × DIM_T, centred frames with reflect padding (torch.stft(center=True)).
    fn stft(&self, x: &[f32], re: &mut [f32], im: &mut [f32]) {
        let n = x.len();
        let reflect = |i: isize| -> f32 {
            let i = if i < 0 {
                -i
            } else if i >= n as isize {
                2 * (n as isize - 1) - i
            } else {
                i
            };
            x[i as usize]
        };
        let mut frame = vec![0f32; N_FFT];
        let mut spec = vec![Complex32::new(0.0, 0.0); N_BINS];
        for t in 0..DIM_T {
            let start = (t * HOP) as isize - TRIM as isize;
            for (k, f) in frame.iter_mut().enumerate() {
                *f = reflect(start + k as isize) * self.win[k];
            }
            let _ = self.fwd.process(&mut frame, &mut spec);
            for f in 0..DIM_F {
                re[f * DIM_T + t] = spec[f].re;
                im[f * DIM_T + t] = spec[f].im;
            }
        }
    }

    /// The model's planes back to CHUNK samples: bins above DIM_F are zero, overlap-add with
    /// the window, divided by the summed squared window (torch.istft(center=True)).
    fn istft(&self, re: &[f32], im: &[f32]) -> Vec<f32> {
        let len = CHUNK + N_FFT;
        let mut out = vec![0f32; len];
        let mut env = vec![0f32; len];
        let mut spec = vec![Complex32::new(0.0, 0.0); N_BINS];
        let mut frame = vec![0f32; N_FFT];
        for t in 0..DIM_T {
            for (f, c) in spec.iter_mut().enumerate() {
                *c = if f < DIM_F {
                    Complex32::new(re[f * DIM_T + t], im[f * DIM_T + t])
                } else {
                    Complex32::new(0.0, 0.0)
                };
            }
            spec[0].im = 0.0;
            spec[N_BINS - 1].im = 0.0;
            let _ = self.inv.process(&mut spec, &mut frame);
            let o = t * HOP;
            for k in 0..N_FFT {
                out[o + k] += frame[k] / N_FFT as f32 * self.win[k];
                env[o + k] += self.win[k] * self.win[k];
            }
        }
        (0..CHUNK)
            .map(|i| {
                let e = env[i + TRIM];
                if e > 1e-8 { out[i + TRIM] / e } else { 0.0 }
            })
            .collect()
    }
}

/// Separate a song into vocals and instrumental, reporting progress (0..1) as it goes.
/// Results are cached by id.
pub fn separate(
    id: &str,
    page_url: &str,
    ytdlp: &str,
    engine: Engine,
    progress: &dyn Fn(f32),
) -> Result<Stems, String> {
    if !crate::paths::valid_media_id(id) {
        return Err("invalid media ID".into());
    }
    if let Some(s) = cached(id, engine) {
        return Ok(s);
    }
    if engine == Engine::Fast {
        for name in ["vocals.int8.onnx", "accompaniment.int8.onnx"] {
            if !models_dir().join("spleeter-int8").join(name).is_file() {
                return Err(
                    "Fast vocal models are missing. See docs/models.md or use HQ separation."
                        .into(),
                );
            }
        }
    }
    progress(0.0);
    let mix = decode(page_url, ytdlp)?;
    let n = mix.len() / 2;
    let vocals = match engine {
        Engine::Hq => mdx(&mix, progress),
        Engine::Fast => spleeter(&mix, progress),
    };
    release_later();
    let vocals = vocals?;
    let stems = stem_paths(id, engine);
    std::fs::create_dir_all(cache_dir()).map_err(|e| e.to_string())?;
    let gain = if engine == Engine::Hq {
        COMPENSATE
    } else {
        1.0
    };
    let voc = |ch: usize, i: usize| vocals[ch][i] * gain;
    write_wav(&stems.vocals.with_extension("part"), n, voc)?;
    write_wav(&stems.instrumental.with_extension("part"), n, |ch, i| {
        mix[2 * i + ch] - voc(ch, i)
    })?;
    std::fs::rename(stems.vocals.with_extension("part"), &stems.vocals)
        .map_err(|e| e.to_string())?;
    std::fs::rename(
        stems.instrumental.with_extension("part"),
        &stems.instrumental,
    )
    .map_err(|e| e.to_string())?;
    prune(16);
    Ok(stems)
}

/// Kim_Vocal_2 (MDX-Net): planar vocals, `n` samples a channel.
fn mdx(mix: &[f32], progress: &dyn Fn(f32)) -> Result<[Vec<f32>; 2], String> {
    let session = session()?;
    let n = mix.len() / 2;
    // Planar, padded: TRIM zeros, the song, zeros to a whole number of chunks, TRIM zeros.
    let pad = GEN - n % GEN;
    let total = TRIM + n + pad + TRIM;
    let mut planes = [vec![0f32; total], vec![0f32; total]];
    for i in 0..n {
        planes[0][TRIM + i] = mix[2 * i];
        planes[1][TRIM + i] = mix[2 * i + 1];
    }
    let fft = Fft::new();
    let chunks = (n + pad) / GEN;
    let mut vocals = [Vec::with_capacity(n + pad), Vec::with_capacity(n + pad)];
    let plane = DIM_F * DIM_T;
    for c in 0..chunks {
        let start = c * GEN;
        // Input: [1, 4, DIM_F, DIM_T] as L.re, L.im, R.re, R.im.
        let mut input = vec![0f32; 4 * plane];
        {
            let (l, r) = input.split_at_mut(2 * plane);
            let (lre, lim) = l.split_at_mut(plane);
            let (rre, rim) = r.split_at_mut(plane);
            // Both channels' transforms side by side.
            std::thread::scope(|sc| {
                sc.spawn(|| fft.stft(&planes[0][start..start + CHUNK], lre, lim));
                fft.stft(&planes[1][start..start + CHUNK], rre, rim);
            });
        }
        let tensor = Tensor::from_array(([1usize, 4, DIM_F, DIM_T], input.into_boxed_slice()))
            .map_err(|e| e.to_string())?;
        let spec: Vec<f32> = {
            let mut s = session.lock().map_err(|_| "model lock poisoned")?;
            let outputs = s
                .run(ort::inputs![tensor])
                .map_err(|e| format!("model failed: {e}"))?;
            let (_, data) = outputs[0]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            data.to_vec()
        };
        let (l, r) = std::thread::scope(|sc| {
            let h = sc.spawn(|| fft.istft(&spec[0..plane], &spec[plane..2 * plane]));
            let r = fft.istft(&spec[2 * plane..3 * plane], &spec[3 * plane..4 * plane]);
            (h.join().unwrap(), r)
        });
        vocals[0].extend_from_slice(&l[TRIM..CHUNK - TRIM]);
        vocals[1].extend_from_slice(&r[TRIM..CHUNK - TRIM]);
        progress((c + 1) as f32 / chunks as f32);
    }

    vocals[0].truncate(n);
    vocals[1].truncate(n);
    Ok(vocals)
}

// ------------------------------------------------------------------ Spleeter (fast)

const S_NFFT: usize = 4096;
const S_HOP: usize = 1024;
const S_BINS: usize = S_NFFT / 2 + 1;
/// The model sees the lowest 1024 bins (up to 11 kHz) in 512-frame segments.
const S_F: usize = 1024;
const S_T: usize = 512;

/// Spleeter's two int8 models (vocals, accompaniment), loaded once, each with half the cores
/// so the two run side by side.
type SessionPair = Arc<(Mutex<Session>, Mutex<Session>)>;

static SPLEETER: Mutex<Option<SessionPair>> = Mutex::new(None);

fn spleeter_sessions() -> Result<SessionPair, String> {
    let mut slot = SPLEETER.lock().map_err(|_| "model lock poisoned")?;
    if let Some(s) = &*slot {
        return Ok(s.clone());
    }
    let dir = models_dir().join("spleeter-int8");
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let load = |f: &str| {
        // No memory arena and no cached memory pattern: the arena keeps a song's worth of
        // activations after every run, which on an 8 GB machine starves the GPU.
        let plain = ort::memory::MemoryInfo::new(
            ort::memory::AllocationDevice::CPU,
            0,
            ort::memory::AllocatorType::Device,
            ort::memory::MemoryType::Default,
        );
        Session::builder()
            .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
            .and_then(|b| b.with_intra_threads((cores / 2).max(1)))
            .and_then(|b| b.with_memory_pattern(false))
            .and_then(|b| match plain {
                Ok(m) => b.with_allocator(m),
                Err(_) => Ok(b),
            })
            .and_then(|b| b.commit_from_file(dir.join(f)))
            .map_err(|e| format!("couldn't load Spleeter ({f}): {e}"))
    };
    let pair = Arc::new((
        Mutex::new(load("vocals.int8.onnx")?),
        Mutex::new(load("accompaniment.int8.onnx")?),
    ));
    *slot = Some(pair.clone());
    Ok(pair)
}

/// Spleeter 2-stems: magnitude spectrogram in, each model estimates its stem's magnitude,
/// and a soft ratio mask (as sherpa-onnx and Spleeter do) carves the vocals out of the mix's
/// complex spectrogram. Returns planar vocals, `n` samples a channel.
fn spleeter(mix: &[f32], progress: &dyn Fn(f32)) -> Result<[Vec<f32>; 2], String> {
    let pair = spleeter_sessions()?;
    let (vs, acc) = (&pair.0, &pair.1);
    let n = mix.len() / 2;
    let mut planner = RealFftPlanner::<f32>::new();
    let fwd = planner.plan_fft_forward(S_NFFT);
    let inv = planner.plan_fft_inverse(S_NFFT);
    let win: Vec<f32> = (0..S_NFFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / S_NFFT as f32).cos())
        .collect();

    // Centre the frames: half an FFT of zeros each side, then round up to a whole hop.
    let half = S_NFFT / 2;
    let frames = (n + 2 * half - S_NFFT).div_ceil(S_HOP) + 1;
    let padded = (frames - 1) * S_HOP + S_NFFT;
    let splits = frames.div_ceil(S_T);
    let tf = splits * S_T;

    // Complex spectrogram per channel: [frame][bin].
    let stft = |ch: usize| -> Vec<Complex32> {
        let mut x = vec![0f32; padded];
        for i in 0..n {
            x[half + i] = mix[2 * i + ch];
        }
        let mut out = vec![Complex32::new(0.0, 0.0); frames * S_BINS];
        let mut frame = vec![0f32; S_NFFT];
        let mut spec = vec![Complex32::new(0.0, 0.0); S_BINS];
        for t in 0..frames {
            for k in 0..S_NFFT {
                frame[k] = x[t * S_HOP + k] * win[k];
            }
            let _ = fwd.process(&mut frame, &mut spec);
            out[t * S_BINS..(t + 1) * S_BINS].copy_from_slice(&spec);
        }
        out
    };
    let (sl, sr) = std::thread::scope(|sc| {
        let h = sc.spawn(|| stft(0));
        let r = stft(1);
        (h.join().unwrap(), r)
    });
    progress(0.15);

    // Model input [2, splits, 512, 1024]: magnitudes, frames past the end zero. Run in
    // batches of a few 512-frame segments (about 12 s of music each) so memory stays small
    // however long the song.
    const BATCH: usize = 1;
    let mut v = vec![0f32; 2 * tf * S_F];
    let mut a = vec![0f32; 2 * tf * S_F];
    let run = |m: &Mutex<Session>, x: Vec<f32>, n: usize| -> Result<Vec<f32>, String> {
        let t = Tensor::from_array(([2usize, n, S_T, S_F], x.into_boxed_slice()))
            .map_err(|e| e.to_string())?;
        let mut s = m.lock().map_err(|_| "model lock poisoned")?;
        let out = s
            .run(ort::inputs![t])
            .map_err(|e| format!("Spleeter failed: {e}"))?;
        let (_, d) = out[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        Ok(d.to_vec())
    };
    let mut first = 0;
    while first < splits {
        let n = BATCH.min(splits - first);
        let mut x = vec![0f32; 2 * n * S_T * S_F];
        for (ch, spec) in [&sl, &sr].into_iter().enumerate() {
            for j in 0..n * S_T {
                let t = first * S_T + j;
                if t >= frames {
                    break;
                }
                let row = &mut x[(ch * n * S_T + j) * S_F..][..S_F];
                for (f, m) in row.iter_mut().enumerate() {
                    *m = spec[t * S_BINS + f].norm();
                }
            }
        }
        // Vocals and accompaniment estimates side by side.
        let x2 = x.clone();
        let (vb, ab) = std::thread::scope(|sc| {
            let h = sc.spawn(|| run(vs, x, n));
            let ab = run(acc, x2, n);
            (h.join().unwrap(), ab)
        });
        let (vb, ab) = (vb?, ab?);
        for ch in 0..2 {
            let src = ch * n * S_T * S_F;
            let dst = (ch * tf + first * S_T) * S_F;
            let len = n * S_T * S_F;
            v[dst..dst + len].copy_from_slice(&vb[src..src + len]);
            a[dst..dst + len].copy_from_slice(&ab[src..src + len]);
        }
        first += n;
        progress(0.15 + 0.7 * first as f32 / splits as f32);
    }
    progress(0.85);

    // Mask, then back to samples.
    let istft = |ch: usize, spec: &[Complex32]| -> Vec<f32> {
        let mut out = vec![0f32; padded];
        let mut env = vec![0f32; padded];
        let mut buf = vec![Complex32::new(0.0, 0.0); S_BINS];
        let mut frame = vec![0f32; S_NFFT];
        for t in 0..frames {
            for f in 0..S_BINS {
                buf[f] = if f < S_F {
                    let i = (ch * tf + t) * S_F + f;
                    let (vv, aa) = (v[i] * v[i], a[i] * a[i]);
                    spec[t * S_BINS + f] * ((vv + 5e-11) / (vv + aa + 1e-10))
                } else {
                    // Above 11 kHz Spleeter has no estimate: leave it to the instrumental.
                    Complex32::new(0.0, 0.0)
                };
            }
            buf[0].im = 0.0;
            buf[S_BINS - 1].im = 0.0;
            let _ = inv.process(&mut buf, &mut frame);
            for k in 0..S_NFFT {
                out[t * S_HOP + k] += frame[k] / S_NFFT as f32 * win[k];
                env[t * S_HOP + k] += win[k] * win[k];
            }
        }
        (0..n)
            .map(|i| {
                let e = env[half + i];
                if e > 1e-8 { out[half + i] / e } else { 0.0 }
            })
            .collect()
    };
    let (l, r) = std::thread::scope(|sc| {
        let h = sc.spawn(|| istft(0, &sl));
        let r = istft(1, &sr);
        (h.join().unwrap(), r)
    });
    progress(1.0);
    Ok([l, r])
}

/// 16-bit stereo WAV at 44.1 kHz.
fn write_wav(
    path: &Path,
    frames: usize,
    sample: impl Fn(usize, usize) -> f32,
) -> Result<(), String> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    let data = (frames * 4) as u32;
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF");
    h.extend_from_slice(&(36 + data).to_le_bytes());
    h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes());
    h.extend_from_slice(&1u16.to_le_bytes());
    h.extend_from_slice(&2u16.to_le_bytes());
    h.extend_from_slice(&RATE.to_le_bytes());
    h.extend_from_slice(&(RATE * 4).to_le_bytes());
    h.extend_from_slice(&4u16.to_le_bytes());
    h.extend_from_slice(&16u16.to_le_bytes());
    h.extend_from_slice(b"data");
    h.extend_from_slice(&data.to_le_bytes());
    f.write_all(&h).map_err(|e| e.to_string())?;
    let mut buf = Vec::with_capacity(frames * 4);
    for i in 0..frames {
        for ch in 0..2 {
            let v = (sample(ch, i).clamp(-1.0, 1.0) * 32767.0) as i16;
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    f.write_all(&buf).map_err(|e| e.to_string())?;
    f.flush().map_err(|e| e.to_string())
}

/// Keep the newest `keep` songs' stems (about 80 MB a song).
fn prune(keep: usize) {
    let Ok(rd) = std::fs::read_dir(cache_dir()) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, PathBuf)> = rd
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "wav"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    files.sort_by_key(|a| std::cmp::Reverse(a.0));
    for (_, p) in files.into_iter().skip(keep * 2) {
        let _ = std::fs::remove_file(p);
    }
}

/// `encore --bench-model`: time the model alone on silence at a few batch sizes and thread
/// counts, to pick the fastest setup for this machine.
pub fn bench() -> Result<(), String> {
    let path = model_path();
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    for threads in [cores, cores.saturating_sub(1).max(1)] {
        let mut s = Session::builder()
            .and_then(|b| b.with_optimization_level(GraphOptimizationLevel::Level3))
            .and_then(|b| b.with_intra_threads(threads))
            .and_then(|b| b.commit_from_file(&path))
            .map_err(|e| e.to_string())?;
        for batch in [1usize, 2] {
            let mk = || {
                Tensor::from_array((
                    [batch, 4, DIM_F, DIM_T],
                    vec![0f32; batch * 4 * DIM_F * DIM_T].into_boxed_slice(),
                ))
                .unwrap()
            };
            let _ = s.run(ort::inputs![mk()]).map(|_| ());
            let t0 = std::time::Instant::now();
            let ok = s.run(ort::inputs![mk()]).is_ok();
            let per = t0.elapsed().as_secs_f32() / batch as f32;
            // Seconds of music per chunk: GEN samples at 44.1 kHz.
            println!(
                "threads {threads} batch {batch}: {per:.2}s per chunk ({:.2}x realtime){}",
                (GEN as f32 / RATE as f32) / per,
                if ok { "" } else { " FAILED" }
            );
        }
    }
    Ok(())
}

/// `encore --probe-model <file>`: print a model's inputs, outputs and metadata.
pub fn probe(path: &str) -> Result<(), String> {
    let s = Session::builder()
        .and_then(|b| b.commit_from_file(path))
        .map_err(|e| e.to_string())?;
    for i in &s.inputs {
        println!("input  {} {:?}", i.name, i.input_type);
    }
    for o in &s.outputs {
        println!("output {} {:?}", o.name, o.output_type);
    }
    let m = s.metadata().map_err(|e| e.to_string())?;
    for k in m.custom_keys().unwrap_or_default() {
        println!(
            "meta   {k} = {}",
            m.custom(&k).ok().flatten().unwrap_or_default()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transform pair must give the signal back, or every stem is smeared.
    #[test]
    fn stft_round_trips() {
        let fft = Fft::new();
        let x: Vec<f32> = (0..CHUNK)
            .map(|i| ((i as f32) * 0.013).sin() * 0.5 + ((i as f32) * 0.0021).cos() * 0.3)
            .collect();
        let plane = DIM_F * DIM_T;
        let (mut re, mut im) = (vec![0f32; plane], vec![0f32; plane]);
        fft.stft(&x, &mut re, &mut im);
        let y = fft.istft(&re, &im);
        // Low-frequency test signal: well under the 3072-bin cut, so it should survive.
        let err = x[TRIM..CHUNK - TRIM]
            .iter()
            .zip(&y[TRIM..CHUNK - TRIM])
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f32::max);
        assert!(err < 1e-3, "max error {err}");
    }
}
