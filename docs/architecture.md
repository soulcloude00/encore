# Architecture

Encore remains one Rust package. Multiple crates are unnecessary at this size;
modules define the boundaries without adding versioning or build complexity.

```text
src/main.rs          Process exit code only
src/lib.rs           Entry point and local panic logging
src/cli.rs           Headless diagnostics and development commands
src/app/mod.rs       App state, queue, events, background deliveries, window startup
src/app/ui.rs        Rune views and input actions
src/app/radio.rs     Station lifecycle, feedback, refills and epochs
src/app/vocals.rs    Filter/stem playback transitions and background separation
src/taste.rs         Pure ranking, feedback scores and artist spacing
src/ytm.rs           YouTube Music metadata adapters
src/mpv.rs           Playback child, IPC status and lifecycle
src/lyrics.rs        LRCLIB adapter and synchronized lyric parsing
src/art.rs           Artwork transformations
src/separate.rs      ONNX inference, transforms and stem cache
src/paths.rs         XDG resolution, model directory and cache-ID validation
src/storage.rs       Atomic settings/history persistence
src/theme.rs         Default palette and optional Omarchy palette following
src/float.rs         Lyrics overlay and optional Hyprland video positioning
```

Views issue application actions. Network work runs on worker threads and sends typed
messages into the application inbox; the poll callback applies them on the UI thread.
Station messages carry an epoch so responses from an old station cannot change a new
queue. Radio responses carry errors as well as successes, so pending work always settles.

mpv owns playback and resolves streams with yt-dlp. Each player has a private temporary
directory for its socket and input bindings; the directory is removed when the player
is dropped. Failed initialization kills and reaps its child. Decode jobs also get
private temporary directories. Media IDs are validated before becoming cache paths.

Settings, history and taste use atomic replacement in the destination directory;
a failed write keeps the previous file. Files retain the existing on-disk format.
Multiple running instances still use last-writer-wins persistence; there is no shared
profile database or single-instance lock yet.

Known boundaries to improve: application state and UI remain sizeable; background
jobs have no general cancellation scheduler; vocal processing loads decoded audio
into memory. These are explicit follow-up work, not guarantees of this preview.
