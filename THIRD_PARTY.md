# Third-party components and services

Encore's MIT license applies to the project's original source and packaging assets.
It does not change the licenses or terms of other components or retrieved content.

- **Rune:** UI framework, pinned to a Git revision in Cargo.toml. Upstream source
  and license: https://github.com/anomalyco/rune
- **Rust dependencies:** exact versions are recorded in Cargo.lock. Their source
  packages include license notices. The release process copies available dependency license
  texts and exports an inventory; maintainers must review obligations before publishing binaries.
- **ONNX Runtime:** used through the `ort` Rust binding. Build-time artifacts may
  be downloaded by `ort-sys`. Version 1.22.0 license and third-party notices
  are preserved under `third_party/onnxruntime/`.
  Upstream: https://github.com/microsoft/onnxruntime
- **mpv, yt-dlp and ffmpeg:** external programs installed by the user, not bundled
  in Encore archives. Their licenses and build options remain independent.
- **Vocal model weights:** optional and not bundled. `Kim_Vocal_2.onnx` is fetched
  from https://github.com/TRvlvr/model_repo when HQ separation is requested.
  Fast Spleeter int8 models must be supplied separately. Their provenance and
  redistribution permissions must be established before bundling them; see docs/models.md.
- **YouTube Music:** unofficial JSON endpoints provide metadata; mpv/yt-dlp resolve
  playback. Encore is not affiliated with or endorsed by YouTube or Google.
- **LRCLIB:** https://lrclib.net supplies lyrics. Music, lyrics and cover artwork
  remain subject to their owners' rights and relevant service terms.

Before publishing a binary, review the dependency inventory and supply all required
license texts/notices. Metadata alone is not a substitute for that review. No media
or model redistribution rights are granted by Encore's source license.
