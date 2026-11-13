# Optional vocal models

Normal playback needs neither models nor ffmpeg. Instant vocal/karaoke filters use
mpv's stereo processing and are approximate; centered instruments can bleed through.
True separation needs ffmpeg and compatible ONNX models.

Run `encore --paths` to see the models directory. Its default is
`~/.local/share/encore/models`; set an absolute `ENCORE_MODEL_DIR` to relocate it.
Run `encore --doctor` to list missing files.

## Fast engine

Supply these files from a source you trust:

```text
models/
  spleeter-int8/
    vocals.int8.onnx
    accompaniment.int8.onnx
```

These are the particular quantized Spleeter exports expected by Encore's current
pipeline, not arbitrary Spleeter checkpoints. This project does not yet provide a
reproducible conversion recipe, verified download, or redistribution license for those
exports. Fast separation is therefore an advanced setup, not a ready-to-use release
feature. Missing files are reported before downloading/decoding a track. The instant
filter remains available when the job fails.

## HQ engine

HQ uses `Kim_Vocal_2.onnx`. If absent, it is automatically downloaded from the URL
recorded as `MODEL_URL` in `src/separate.rs` when an HQ job starts. This is a large
third-party download; normal playback does not trigger it. To control acquisition,
place a trusted copy at `models/Kim_Vocal_2.onnx` before using HQ mode.

The UI uses fast stems first when installed, and queues HQ stems when that engine is
selected. On a fresh setup without fast models, selecting HQ queues HQ directly.
The HQ command can run directly without the fast models:

```sh
encore --separate VIDEO_ID hq
```

Weights are not included in release archives. Their terms are separate from Encore's
MIT license. Before redistributing any model, establish its source and permissions.
The current HQ downloader has no pinned integrity digest; verified model distribution
and the fast conversion recipe remain release follow-ups.

Stems are cached under the cache directory; deleting that cache forces recomputation.
Separation is memory intensive and can occupy multiple CPU cores.
