//! Headless diagnostics and developer tools, before any GPU/window initialization.
use crate::{mpv, paths, separate};

const HELP: &str = "Encore — desktop music player

Usage: encore [COMMAND]

  --help, -h                 Show this help
  --version, -V              Show version
  --doctor                   Check playback tools and optional model files
  --paths                    Show configuration, cache, and model directories
  --separate ID [fast|hq]     Separate a track into cached stems
  --probe-model FILE         Inspect an ONNX model
  --bench-model              Benchmark the installed HQ model

Launch without a command to open the player.
ENCORE_YTDLP overrides yt-dlp; ENCORE_MODEL_DIR sets an absolute model directory.
XDG_CONFIG_HOME, XDG_CACHE_HOME and XDG_DATA_HOME control local storage.";

pub fn run(args: Vec<String>) -> Result<bool, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    match command {
        "--help" | "-h" => println!("{HELP}"),
        "--version" | "-V" => println!("Encore {}", env!("CARGO_PKG_VERSION")),
        "--paths" => {
            paths::report("Config", &paths::config());
            paths::report("Cache", &paths::cache());
            paths::report("Models", &paths::models());
        }
        "--doctor" => doctor()?,
        "--separate" => {
            let (id, engine) = separation_args(&args[1..])?;
            let t0 = std::time::Instant::now();
            let stems = separate::separate(
                id,
                &format!("https://music.youtube.com/watch?v={id}"),
                &mpv::ytdl_path(),
                engine,
                &|p| {
                    eprint!("\r{:>3.0}%  {:.1}s", p * 100.0, t0.elapsed().as_secs_f32());
                },
            )?;
            eprintln!();
            println!(
                "{}\n{}",
                stems.vocals.display(),
                stems.instrumental.display()
            );
        }
        "--probe-model" => {
            if args.len() != 2 {
                return Err("usage: encore --probe-model FILE".into());
            }
            separate::probe(&args[1])?;
        }
        "--bench-model" => separate::bench()?,
        // Rune has its own scene/screenshot arguments; pass these through.
        _ => return Ok(false),
    }
    Ok(true)
}

fn separation_args(args: &[String]) -> Result<(&str, separate::Engine), String> {
    if args.is_empty() || args.len() > 2 || !paths::valid_media_id(&args[0]) {
        return Err(
            "usage: encore --separate ID [fast|hq]; ID must contain only letters, numbers, _ or -"
                .into(),
        );
    }
    let engine = match args.get(1).map(String::as_str).unwrap_or("fast") {
        "fast" => separate::Engine::Fast,
        "hq" => separate::Engine::Hq,
        _ => return Err("engine must be fast or hq".into()),
    };
    Ok((&args[0], engine))
}

fn tool(program: &str, flag: &str) -> bool {
    match std::process::Command::new(program).arg(flag).output() {
        Ok(out) if out.status.success() => {
            println!(
                "OK {program}: {}",
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .next()
                    .unwrap_or("installed")
            );
            true
        }
        _ => {
            println!("MISSING or unusable: {program}");
            false
        }
    }
}

fn supported_mpv(text: &str) -> bool {
    let version = text
        .split_whitespace()
        .nth(1)
        .unwrap_or("")
        .trim_start_matches('v');
    let mut parts = version.split('.');
    matches!((parts.next().and_then(|x| x.parse::<u32>().ok()), parts.next().and_then(|x| x.parse::<u32>().ok())), (Some(major), Some(minor)) if major > 0 || minor >= 38)
}

fn doctor() -> Result<(), String> {
    let mpv = tool("mpv", "--version")
        && std::process::Command::new("mpv")
            .arg("--version")
            .output()
            .is_ok_and(|out| supported_mpv(&String::from_utf8_lossy(&out.stdout)));
    if !mpv {
        println!("Encore requires mpv 0.38 or newer.");
    }
    let ytdlp = tool(&mpv::ytdl_path(), "--version");
    let ffmpeg = tool("ffmpeg", "-version");
    if !ffmpeg {
        println!("ffmpeg is optional, required for stem separation.");
    }
    for file in [
        "spleeter-int8/vocals.int8.onnx",
        "spleeter-int8/accompaniment.int8.onnx",
        "Kim_Vocal_2.onnx",
    ] {
        let path = paths::models().join(file);
        println!(
            "{} {}",
            if path.is_file() {
                "OK"
            } else {
                "OPTIONAL missing:"
            },
            path.display()
        );
    }
    println!(
        "Model setup: docs/models.md. Desktop/GPU and network playback are not tested by --doctor."
    );
    if mpv && ytdlp {
        Ok(())
    } else {
        Err("install mpv and yt-dlp, then rerun encore --doctor".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_separation_before_touching_disk_or_network() {
        for args in [
            vec![],
            vec!["../escape".into()],
            vec!["abc".into(), "typo".into()],
        ] {
            assert!(separation_args(&args).is_err());
        }
        let args = vec!["abc_123".into(), "hq".into()];
        assert_eq!(separation_args(&args).unwrap().1, separate::Engine::Hq);
    }
    #[test]
    fn checks_the_mpv_protocol_minimum() {
        assert!(!supported_mpv("mpv 0.37.0"));
        assert!(supported_mpv("mpv 0.38.0"));
        assert!(supported_mpv("mpv 0.41.0-dev"));
        assert!(supported_mpv("mpv v0.41.0 Copyright"));
        assert!(!supported_mpv("garbage"));
    }
}
