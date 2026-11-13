# Encore

A Linux desktop music player with YouTube Music search, continuous song radio,
synced lyrics, video playback, and optional local vocal separation. Built in Rust
with [Rune](https://github.com/anomalyco/rune); audio plays through mpv.

**Early preview (0.1.0).** Playback uses unofficial YouTube Music endpoints and
can break when the service changes. No YouTube login is required or supported.
Linux is the current target; Windows and macOS are not supported by this code.
Omarchy is optional: other desktops get Encore's default dark theme.

## Install

### From a release archive

Download the archive matching your CPU from this repository's Releases page.
Published binaries are built on Ubuntu 24.04 for x86_64 and ARM64, and need a
compatible glibc/libstdc++ runtime, `libxkbcommon`, fonts, and a working GPU driver.
They are not static binaries. See `DEPENDENCIES.txt` in each archive.

Install `mpv` (0.38 or newer) and a recent `yt-dlp` first. Distro packages
may be older; use a maintained newer package when needed. On Debian/Ubuntu:

```sh
sudo apt install mpv libxkbcommon0 fontconfig fonts-dejavu-core
# Install yt-dlp using its official installation instructions, then keep it updated.
```

Verify the downloaded archive before extracting it:

```sh
sha256sum -c encore-0.1.0-<target>.tar.gz.sha256  # run beside the downloaded archive
```

After extracting and entering the directory:

```sh
./install.sh --binary bin/encore
~/.local/bin/encore --doctor
```

The checksum command uses a placeholder: replace `<target>` with the target in the
downloaded filename. Checksums detect corruption; obtain both files from the same
trusted release. Open Encore from your desktop launcher or run `~/.local/bin/encore`.

### From source

Install Rust via rustup, a C/C++ compiler, Git, pkg-config, OpenSSL development
headers and xkbcommon development headers. The toolchain file pins the Rust version;
Cargo.lock and Rune's Git revision are committed for repeatable dependency selection.
The first build downloads dependencies, including ONNX Runtime's build artifacts.

```sh
# Debian/Ubuntu build dependencies
sudo apt install build-essential pkg-config libssl-dev libxkbcommon-dev python3
# Arch Linux: base-devel openssl libxkbcommon python
cargo run --release --locked -- --doctor
./install.sh
```

Source builds default to two jobs during installation to limit memory use. Set
`CARGO_BUILD_JOBS` to change this. `python3` is needed by the installer and packaging.

`yt-dlp` is selected in this order: `ENCORE_YTDLP`, `~/.local/bin/yt-dlp`, then PATH.
See the [official yt-dlp installation guide](https://github.com/yt-dlp/yt-dlp#installation).
A stale yt-dlp is a common reason tracks fail to start.

Uninstall with `./install.sh --uninstall` from the source tree or extracted release.
Settings and cached data are kept. `ENCORE_PREFIX=/absolute/path` changes the install
prefix (default `~/.local`); use the same prefix to uninstall.

## Use

Search and press Enter, then choose a song. Search results become the queue.
Home shows recommendations and daily mixes; song radio continues with similar tracks.
Likes and listening feedback shape future recommendations on this device.

| Key | Action |
| --- | --- |
| Space | Play/pause |
| Left / Right | Seek 10 seconds |
| Up / Down | Volume |
| N / P | Next / previous |
| S | Shuffle upcoming tracks |
| R | Cycle repeat |
| / | Focus search |
| Esc | Leave search |

Drag the progress line to seek and the volume slider to adjust volume. Lyrics,
artwork styles, video, and vocal controls are available in the interface.

## Optional vocals and karaoke

Instant vocal filters work through mpv. Real stem separation additionally needs
`ffmpeg` and model files; fast models are **not bundled**. HQ separation downloads
its model when first requested. Read [model setup](docs/models.md) before using it.
Missing models leave the instant filter available and report an error.

## Local files and privacy

Run `encore --paths` to see exact locations and `encore --doctor` to check tools.

| Data | Default path |
| --- | --- |
| Settings, history, likes and listening statistics | `~/.config/encore/` |
| Artwork/stem processing cache and crash log | `~/.cache/encore/` |
| Optional model weights | `~/.local/share/encore/models/` |

`XDG_CONFIG_HOME`, `XDG_CACHE_HOME`, and `XDG_DATA_HOME` override these roots.
Overrides must be absolute; empty/relative XDG values are ignored. An absolute
`ENCORE_MODEL_DIR` overrides the models directory. Existing Encore files stay compatible.

Searches and playback requests go to YouTube/Google services; lyrics requests go
to LRCLIB; artwork is fetched from URLs in search responses. Recommendations are
ranked locally from listening history. There is no Encore account or analytics
service. Crash logs are local and can contain paths; review them before sharing.

## Contribute

See [CONTRIBUTING.md](CONTRIBUTING.md), [architecture](docs/architecture.md),
[recommendation behavior](docs/algorithms.md), and the [release guide](docs/releasing.md).
Report playback issues with the Encore, mpv and yt-dlp versions and the desktop/session
used. `--doctor` checks tools; it does not verify the network, GPU, or live playback.

Encore's code is MIT licensed. Dependencies, model weights, music, lyrics and artwork
have separate terms; see [THIRD_PARTY.md](THIRD_PARTY.md).
