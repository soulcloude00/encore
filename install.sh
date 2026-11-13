#!/usr/bin/env bash
# Build Encore's release binary and install it for this user, with an app-launcher entry.
# Rerun after changing the code. Uninstall with: ./install.sh --uninstall
set -euo pipefail
cd "$(dirname "$0")"

bin="$HOME/.local/lib/encore/encore"
launcher="$HOME/.local/bin/encore"
desktop="$HOME/.local/share/applications/encore.desktop"
icon="$HOME/.local/share/icons/hicolor/scalable/apps/encore.svg"

if [[ ${1:-} == --uninstall ]]; then
  rm -f "$bin" "$launcher" "$desktop" "$icon"
  rmdir "$(dirname "$bin")" 2>/dev/null || true
  update-desktop-database -q "$HOME/.local/share/applications" 2>/dev/null || true
  echo "Encore removed (settings stay in ~/.config/encore)."
  exit 0
fi

# -j3 and a low priority keep the build from starving the desktop of memory.
nice -n 10 cargo build --release -j3

install -Dm755 target/release/encore "$bin"
# The launcher reopens Encore after a crash (see packaging/encore-launch).
install -Dm755 packaging/encore-launch "$launcher"
install -Dm644 packaging/encore.svg "$icon"
install -Dm644 packaging/encore.desktop "$desktop"
# Absolute Exec, so the launcher works even where ~/.local/bin isn't on PATH.
sed -i "s|^Exec=.*|Exec=$launcher|" "$desktop"
update-desktop-database -q "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true

echo "Installed: $launcher (runs $bin)"
echo "Open it from the app launcher (Super+Space → Encore)."
