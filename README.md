# Encore

A YouTube Music player for the desktop. The UI is built with [Rune](https://github.com/anomalyco/rune), and audio plays through mpv.

## Run

```sh
cargo run --release
```

It needs `mpv` and `yt-dlp` installed. YouTube breaks old yt-dlp versions often, which shows up as songs that never start. Install a per-user copy that's easy to keep current:

```sh
uv tool install yt-dlp     # later: uv tool upgrade yt-dlp
```

Encore uses `~/.local/bin/yt-dlp` when it exists. Otherwise it uses the `yt-dlp` on your PATH, and you can set `ENCORE_YTDLP` to use a different one.

## Use

- Type a search and press Enter, then click a song. The rest of the results play after it.
- **Space** plays or pauses, **← / →** seeks 10 seconds, **↑ / ↓** changes the volume, **N / P** skips to the next or previous song, **S** turns shuffle on or off, **R** cycles repeat, **/** focuses search, and **Esc** leaves the search box.
- Drag the progress line in the top bar to seek, and drag the slider on the right to change the volume.

## How it works

- `src/ytm.rs` searches YouTube Music through the web client's JSON endpoint. It gets title, artist, album, duration and cover art.
- `src/mpv.rs` runs a headless mpv and controls it over its JSON IPC socket. mpv uses yt-dlp to resolve the stream.
- `src/ui.rs` draws the whole interface. `src/main.rs` holds the state, the queue and the key handling.

Rune's own audio `Player` only works on macOS so far, which is why playback goes through mpv.
This uses an unofficial endpoint and yt-dlp, so treat it as a personal-use project.
