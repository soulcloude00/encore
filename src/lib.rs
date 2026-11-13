//! Encore's application entry point and internal services.
mod app;
mod art;
mod cli;
mod float;
mod lyrics;
mod mpv;
mod paths;
mod separate;
mod storage;
mod taste;
mod theme;
mod ytm;

pub(crate) use app::{App, MOODS, Msg, Page, Quality, Repeat, Vocal, local_hour};

/// Run a diagnostic command or start the desktop application.
pub fn run() -> std::process::ExitCode {
    crash_log();
    match cli::run(std::env::args().skip(1).collect()) {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => {
            app::run();
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("encore: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// Write any crash, with its backtrace, to ~/.cache/encore/crash.log (and stderr).
fn crash_log() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let bt = std::backtrace::Backtrace::force_capture();
        let path = separate::cache_dir().parent().map(|d| d.join("crash.log"));
        if let Some(path) = path {
            let _ = std::fs::create_dir_all(path.parent().unwrap());
            let free = std::fs::read_to_string("/proc/meminfo")
                .ok()
                .and_then(|m| {
                    m.lines()
                        .find(|l| l.starts_with("MemAvailable"))
                        .map(str::to_string)
                })
                .unwrap_or_default();
            let _ = std::fs::write(&path, format!("{info}\n{free}\n\n{bt}\n"));
        }
        default(info);
    }));
}
