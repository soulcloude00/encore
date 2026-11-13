# Changelog

## 0.1.1 — Preview

- Accept mpv version strings both with and without the `v` prefix in diagnostics.

## 0.1.0 — Initial release candidate

- Desktop YouTube Music search and mpv playback, queue, shuffle and repeat.
- Local recommendation profile, daily mixes, song radio, lyrics and artwork styles.
- Optional vocal/karaoke separation and optional Omarchy theme integration.
- Split application and service modules; central XDG paths and atomic profile writes.
- Add headless help, version, dependency diagnostics and path reporting.
- Fix radio failures, endless exhausted-station requests, unstable ranking ties,
  duplicate source votes and uploads, queue-boundary artist spacing, and feedback
  attribution when replacing the queue.
- Validate media cache IDs and use private temporary playback/decode directories.
- Add Linux archive packaging, checksums, CI and a draft prerelease workflow.
