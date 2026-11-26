#!/usr/bin/env bash
# Render the macOS and Linux app icons from the Pdfrp master vector, assets/pdfrp/simge.svg.
# The Windows icon (assets/pdfrp/pdfrp.ico) is drawn by hand and left alone.
#
# Needs: resvg (brew install resvg / cargo install resvg). On macOS, iconutil also writes the .icns.
# Run it before packaging for macOS, Linux or FreeBSD: those packages read the files it writes
# (assets/pdfrp/hicolor/, pdfrp-1024.png, pdfrp.icns). The Windows build needs none
# of them. After committing new outputs, record them in ATTRIBUTION.toml, then
# `cargo xtask assets --write`.
#
#   packaging/icons.sh
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="$ROOT/assets/pdfrp"
SVG="$DIR/simge.svg"
ID="local.migez.MigEzPdf"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

command -v resvg >/dev/null || { echo "error: resvg not found (brew install resvg)" >&2; exit 1; }

# The master fills its square: right for Linux. macOS icons follow Apple's grid instead: an
# 824 px body centred on a transparent 1024 canvas.
MAC="$TMP/macos.svg"
{
  echo '<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" viewBox="0 0 1024 1024">'
  echo '<image x="100" y="100" width="824" height="824" xlink:href="'"$SVG"'"/>'
  echo '</svg>'
} >"$MAC"

render() {
  local src="$1" size="$2" out="$3"
  resvg -w "$size" -h "$size" "$src" "$out" </dev/null
}

# 1024 px PNG on Apple's grid.
render "$MAC" 1024 "$DIR/pdfrp-1024.png"

# Linux hicolor theme.
for s in 16 24 32 48 64 128 256 512; do
  mkdir -p "$DIR/hicolor/${s}x${s}/apps"
  render "$SVG" "$s" "$DIR/hicolor/${s}x${s}/apps/$ID.png"
done
mkdir -p "$DIR/hicolor/scalable/apps"
cp "$SVG" "$DIR/hicolor/scalable/apps/$ID.svg"

# macOS .icns.
if command -v iconutil >/dev/null; then
  SET="$TMP/pdfrp.iconset"
  mkdir -p "$SET"
  for s in 16 32 128 256 512; do
    render "$MAC" "$s" "$SET/icon_${s}x${s}.png"
    render "$MAC" $((s * 2)) "$SET/icon_${s}x${s}@2x.png"
  done
  iconutil -c icns -o "$DIR/pdfrp.icns" "$SET"
else
  echo "warning: iconutil not found (macOS only); pdfrp.icns not written" >&2
fi
echo "icons written to $DIR"
