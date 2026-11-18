#!/usr/bin/env bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# Regenerates the SuperTuxKart showcase banner from this folder, deterministically (the same bytes every run):
#   ./build.sh            banner.cbmd and banner.json from assets/ and source/ (the scene is crates/gamemeta-fixtures/src/stk_banner.rs)
#   ./build.sh --assets   first re-fetches SuperTuxKart's files (CREDITS.md), re-exports the meshes with Blender and re-sizes
#                         the textures into source/ and assets/ (needs curl, ImageMagick, Blender 4+)
# Then recapture the HOME reference: scripts/azahar/capture (see scripts/azahar/README.md).
set -euo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../../.." && pwd)

if [ "${1:-}" = "--assets" ]; then
  SVN=https://svn.code.sf.net/p/supertuxkart/code
  GIT=https://raw.githubusercontent.com/supertuxkart/stk-code/master
  DL=$(mktemp -d)
  get() { curl -sfL -A "asset-fetch/1.0" -o "$DL/$2" "$1"; }
  for f in tuxkart.png tux_body.png engine.png wheel.png tux_shadow.png; do get "$SVN/stk-assets/karts/tux/$f" "$f"; done
  get "$SVN/media/trunk/karts/tux/tux.blend" tux.blend
  get "$SVN/media/trunk/models/gift-box.blend" gift-box.blend
  get "$SVN/media/trunk/models/gift-box.png" gift-box.png
  for f in racetrack_asphalt.jpg racetrack_grass.jpg sky2.jpg pino.png nitro-particle.png item_spark.png flaring-star.png \
    city_checker.png smoke.png; do get "$SVN/stk-assets/textures/$f" "$f"; done
  get "$GIT/data/gui/icons/logo_slim.png" logo_slim.png
  # name and size of each texture in assets/ (the scene picks the PICA format: stk_banner.rs TEXTURES)
  while read -r name src size; do
    convert "$DL/$src" -resize "$size!" -strip -define png:exclude-chunks=date,time,tIME -depth 8 "PNG32:$HERE/assets/$name.png"
  done <<'EOF'
tuxkart tuxkart.png 256x256
tux_body tux_body.png 256x256
engine engine.png 128x128
wheel wheel.png 64x64
tux_shadow tux_shadow.png 64x64
racetrack_asphalt racetrack_asphalt.jpg 128x64
racetrack_grass racetrack_grass.jpg 64x64
sky2 sky2.jpg 128x64
pino pino.png 128x128
nitro-particle nitro-particle.png 64x64
item_spark item_spark.png 64x64
flaring-star flaring-star.png 64x64
gift-box gift-box.png 64x32
city_checker city_checker.png 32x32
smoke smoke.png 64x64
logo_slim logo_slim.png 128x64
EOF
  blender -b "$DL/tux.blend" --python "$HERE/source/export_blend.py" -- "$HERE/source/tux-kart.mesh" 20 >/dev/null
  blender -b "$DL/gift-box.blend" --python "$HERE/source/export_blend.py" -- "$HERE/source/gift-box.mesh" >/dev/null
  echo "fetched into $DL (delete it when done)" >&2
fi

cd "$REPO"
cargo run -q -p gamemeta-fixtures --bin write-stk-banner
GM_WRITE_GOLDEN=1 cargo test -q -p gamemeta-fixtures --test stk_banner the_banner_dumps_to_the_golden_scene >/dev/null
sha256sum "$HERE/banner.cbmd"
