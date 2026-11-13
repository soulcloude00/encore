# Releasing Encore

The repository builds Linux preview archives for x86_64 and ARM64. CI does not publish
an approved release automatically: a version tag produces a draft prerelease.

1. Update Cargo.toml's version, regenerate Cargo.lock if necessary, and update CHANGELOG.md.
2. Run `scripts/check.sh` and test actual desktop playback and radio on intended targets.
3. Run `cargo build --release --locked`, then `scripts/package.sh` to inspect an archive.
4. Extract to a clean directory, install with `./install.sh --binary bin/encore`, run
   `encore --doctor`, and verify playback, seek, queue, lyrics, restart, and uninstall.
5. Review third-party license obligations and include required license texts/notices.
   The GitHub workflow exports Cargo metadata as an inventory for this review; model
   weights and external playback programs are not bundled.
6. Push an annotated tag matching the manifest, for example `v0.1.0`. Both CI builds
   must pass before the workflow creates a **draft prerelease** with archives/checksums.
7. Inspect CI logs, extracted archives, dependency inventory, and checksums. Add tested
   distros/architectures, known issues, and model setup limitations to the release notes.
8. Publish the prerelease only after review. A stable release needs successful tests
   on users' systems, not just a green compilation job.

Use workflow_dispatch to build downloadable CI artifacts before tagging. Action
runners use Ubuntu 24.04; binaries require compatible glibc/libstdc++, xkbcommon and
working display/GPU libraries. Do not describe them as distro-independent.

The package script accepts `scripts/package.sh TARGET BINARY`. It refuses unsupported
target names, unresolved dynamic libraries, and adjacent shared ONNX Runtime builds,
which would require additional loader/bundling work. Native CI builds select each
architecture explicitly. Archives include install/uninstall scripts, desktop metadata,
icon, docs, MIT license, dependency metadata and checksums.
