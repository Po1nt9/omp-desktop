#!/usr/bin/env bash
# Generate Tauri app icons + tray icons from a single original master.
#   All artwork (app dock/.exe/.icns + menu-bar tray)  ←  src-tauri/icons/omp-mark.svg
# OMP Desktop ships one original black/white/orange geometric mark; do not
# reintroduce per-surface sources.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ICONS="$ROOT/src-tauri/icons"
SVG="$ICONS/omp-mark.svg"

if [[ ! -f "$SVG" ]]; then
  echo "Missing master mark: $SVG" >&2
  exit 1
fi

command -v sips >/dev/null || { echo "sips required (macOS)" >&2; exit 1; }
command -v iconutil >/dev/null || { echo "iconutil required (macOS)" >&2; exit 1; }
command -v python3 >/dev/null || { echo "python3 required" >&2; exit 1; }

# ── Rasterize the SVG master to a 1024px RGBA source ────────────────────────
MASTER="$ICONS/icon-source.png"
sips -s format png "$SVG" --out "$MASTER" >/dev/null
# sips keeps intrinsic SVG size; normalize to 1024 so every derivative is sharp.
sips -z 1024 1024 "$MASTER" >/dev/null

# App icons (full-color artwork)
sips -z 512 512 "$MASTER" --out "$ICONS/icon.png" >/dev/null
sips -z 32 32 "$MASTER" --out "$ICONS/32x32.png" >/dev/null
sips -z 64 64 "$MASTER" --out "$ICONS/64x64.png" >/dev/null
sips -z 128 128 "$MASTER" --out "$ICONS/128x128.png" >/dev/null
sips -z 256 256 "$MASTER" --out "$ICONS/128x128@2x.png" >/dev/null

# .icns via iconutil
ICONSET="$ICONS/AppIcon.iconset"
rm -rf "$ICONSET"
mkdir -p "$ICONSET"
for pair in \
  "16 icon_16x16.png" \
  "32 icon_16x16@2x.png" \
  "32 icon_32x32.png" \
  "64 icon_32x32@2x.png" \
  "128 icon_128x128.png" \
  "256 icon_128x128@2x.png" \
  "256 icon_256x256.png" \
  "512 icon_256x256@2x.png" \
  "512 icon_512x512.png" \
  "1024 icon_512x512@2x.png"
do
  set -- $pair
  sips -z "$1" "$1" "$MASTER" --out "$ICONSET/$2" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$ICONS/icon.icns"
rm -rf "$ICONSET"

# .ico — Windows multi-resolution icon via PIL
python3 - <<'PY' "$ICONS" "$MASTER"
import sys
from pathlib import Path
from PIL import Image

icons, master = Path(sys.argv[1]), Path(sys.argv[2])
src = Image.open(master).convert("RGBA")
sizes = [256, 128, 64, 48, 32, 24, 16]
frames = [src.resize((s, s), Image.Resampling.LANCZOS) for s in sizes]
frames[0].save(icons / "icon.ico", format="ICO", sizes=[(s, s) for s in sizes], append_images=frames[1:])
PY

# ── Tray / menu-bar from the same master ─────────────────────────────────────
# tray-icon crate sizes the NSImage to 18pt tall. Embed 36px (@2x) so retina
# is sharp. The OMP mark already has its own dark rounded field, so render it
# faithfully without forcing a flat-black silhouette.
python3 - <<'PY' "$ICONS" "$MASTER"
from pathlib import Path
import sys
from PIL import Image

icons, master = Path(sys.argv[1]), Path(sys.argv[2])
src = Image.open(master).convert("RGBA")

outs = {
    "tray-icon.png": (36, 0.0),
    "tray-icon@2x.png": (36, 0.0),
    "tray-icon-18.png": (18, 0.0),
    "tray-16.png": (16, 0.0),
    "tray-32.png": (32, 0.0),
    "tray-source.png": (128, 0.0),
}
for name, (sz, pad_ratio) in outs.items():
    inner = max(1, int(round(sz * (1.0 - 2 * pad_ratio))))
    resized = src.resize((inner, inner), Image.Resampling.LANCZOS)
    canvas = Image.new("RGBA", (sz, sz), (0, 0, 0, 0))
    ox = oy = (sz - inner) // 2
    canvas.alpha_composite(resized, (ox, oy))
    canvas.save(icons / name, "PNG")
    n = sum(1 for p in canvas.getdata() if p[3] > 20)
    print(f"{name}: {sz}x{sz} opaque={n}")
    if sz <= 36 and n < 40:
        raise SystemExit(f"{name} looks too empty (opaque={n})")
PY

# Public / assets logo (square app mark for web surfaces)
cp "$ICONS/icon.png" "$ROOT/public/logo.png"
cp "$ICONS/128x128@2x.png" "$ROOT/assets/logo.png"

echo "OK — all artwork generated from: omp-mark.svg"
echo "dock/exe: icon*.png/icns/ico; tray: tray-*.png; web: public/logo.png, assets/logo.png"
