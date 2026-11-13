# Contributing

Start with an issue describing the behavior you want to change. Small fixes can go
straight to a pull request. Explain the trigger, resulting behavior, and validation.

Install the source build dependencies in README.md, then run:

```sh
rustup component add rustfmt clippy
scripts/check.sh
cargo run --locked
```

The checks cover formatting, Clippy, unit tests, and shell syntax. Tests run without
a display, model weights, or live music services. Keep algorithm tests deterministic;
use synthetic tracks and explicit feedback rather than live YouTube responses.

Before a playback change, manually exercise search, play/pause, seek, next/previous,
queue replacement, repeat, shuffle, failed playback, and song radio. Before a UI
change, check a small window and both the default theme and any desktop integration
it touches. State which desktop and architecture you tested.

Keep application transitions in `src/app/`, ranking in `src/taste.rs`, persistence
in `src/storage.rs`, and filesystem policy in `src/paths.rs`. External responses belong
in service adapters. Don't add network calls to view rendering or commit personal
history, music, credentials, caches, model weights, or build outputs.

Contributions are made under the project's MIT license. Third-party assets need
attribution and compatible redistribution rights.
