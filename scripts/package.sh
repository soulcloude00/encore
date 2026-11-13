#!/usr/bin/env bash
# Package an already-built binary. CI builds on each supported runner first.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
version=$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["package"]["version"])')
target=${1:-$(rustc -vV | sed -n 's/^host: //p')}
case "$target" in x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu) ;; *) echo "Unsupported release target: $target" >&2; exit 2 ;; esac
binary=${2:-"${CARGO_TARGET_DIR:-target}/release/encore"}
[[ -x $binary ]] || { echo "Build the release binary first: $binary" >&2; exit 1; }
python3 - "$binary" "$target" <<'PY'
import pathlib, struct, sys
with pathlib.Path(sys.argv[1]).open('rb') as binary:
    header = binary.read(20)
assert header[:4] == b'\x7fELF', 'release binary must be ELF'
byteorder = '<' if header[5] == 1 else '>'
machine = struct.unpack(byteorder + 'H', header[18:20])[0]
expected = {'x86_64-unknown-linux-gnu': 62, 'aarch64-unknown-linux-gnu': 183}[sys.argv[2]]
assert machine == expected, 'binary architecture does not match release target'
PY
mkdir -p dist
stage=$(mktemp -d)
trap 'rm -rf -- "$stage"' EXIT
name="encore-$version-$target"
mkdir -p "$stage/$name/bin"
install -m755 "$binary" "$stage/$name/bin/encore"
cp -a packaging "$stage/$name/"
cp install.sh README.md LICENSE THIRD_PARTY.md CHANGELOG.md "$stage/$name/"
cp -a docs "$stage/$name/"
cargo metadata --locked --format-version 1 --filter-platform "$target" > "$stage/$name/CARGO-DEPENDENCIES.json"
python3 scripts/collect-licenses.py "$stage/$name/CARGO-DEPENDENCIES.json" "$stage/$name/licenses"
cp -a third_party "$stage/$name/"
# Record dynamic dependencies; do not claim a static or universally portable binary.
ldd "$binary" > "$stage/$name/DEPENDENCIES.txt"
if rg -q 'not found' "$stage/$name/DEPENDENCIES.txt"; then echo "Binary has unresolved shared libraries." >&2; exit 1; fi
if [[ -n $(find "$(dirname -- "$binary")" -maxdepth 1 -name 'libonnxruntime.so*' -print -quit) ]]; then
  echo "This build requires a shared ONNX Runtime; use a statically linked build before distributing." >&2
  exit 1
fi
tar -C "$stage" -czf "dist/$name.tar.gz" "$name"
(cd dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
echo "dist/$name.tar.gz"
