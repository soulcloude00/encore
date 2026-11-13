#!/usr/bin/env bash
# Per-user source or release-archive installation. Never needs sudo.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"

usage() { echo "Usage: ./install.sh [--binary FILE | --uninstall | --help]"; }
mode=source
binary=
case ${1:-} in
  --help|-h) usage; exit 0 ;;
  --uninstall) mode=uninstall; shift ;;
  --binary) [[ $# == 2 ]] || { usage >&2; exit 2; }; mode=binary; binary=$2; shift 2 ;;
  '') ;;
  *) usage >&2; exit 2 ;;
esac
[[ $# == 0 ]] || { usage >&2; exit 2; }
[[ $(uname -s) == Linux ]] || { echo "Encore currently supports Linux." >&2; exit 1; }
prefix=${ENCORE_PREFIX:-"$HOME/.local"}
[[ $prefix == /* ]] || { echo "ENCORE_PREFIX must be absolute." >&2; exit 2; }
bin="$prefix/lib/encore/encore"
launcher="$prefix/bin/encore"
desktop="$prefix/share/applications/encore.desktop"
icon="$prefix/share/icons/hicolor/scalable/apps/encore.svg"
if [[ $mode == uninstall ]]; then
  rm -f -- "$bin" "$launcher" "$desktop" "$icon"
  rmdir -- "$(dirname -- "$bin")" 2>/dev/null || true
  echo "Encore removed. Your settings, history, and cached models were kept."
  exit 0
fi
command -v python3 >/dev/null || { echo "Install python3 to run the installer." >&2; exit 1; }
if [[ $mode == source ]]; then
  cargo build --release --locked --jobs "${CARGO_BUILD_JOBS:-2}"
  binary="${CARGO_TARGET_DIR:-target}/release/encore"
fi
[[ -f $binary && -x $binary ]] || { echo "Missing executable: $binary" >&2; exit 1; }
install -Dm755 -- "$binary" "$bin"
install -Dm755 packaging/encore-launch "$launcher"
install -Dm644 packaging/encore.svg "$icon"
install -Dm644 packaging/encore.desktop "$desktop"
# Desktop Exec uses its own quoting rules; escape reserved characters in the absolute path.
python3 - "$desktop" "$launcher" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
launcher = sys.argv[2]
for ch in ('\\', '"', '`', '$'):
    launcher = launcher.replace(ch, '\\' + ch)
launcher = launcher.replace('%', '%%')
# Desktop-entry string parsing consumes one escape layer before Exec is parsed.
launcher = launcher.replace('\\', '\\\\')
path.write_text(path.read_text().replace('Exec=encore', f'Exec="{launcher}"'))
PY
update-desktop-database -q "$prefix/share/applications" 2>/dev/null || true
echo "Installed: $launcher"
echo "Run '$launcher --doctor' to check playback dependencies."
