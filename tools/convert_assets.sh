#!/bin/bash
# Regenerates assets/ from the original 2003 DarkBASIC Pro project.
#
#   tools/convert_assets.sh [path/to/original/Dark Messenger] [path/to/playable demo]
#
# Models (.x) go through tools/xconv into .glb, lightmaps and lightning
# frames (.bmp) become .png, the ambient wind track becomes .ogg and sound
# effects 16-bit .wav. The first-person hands and dagger only exist in the
# later "Dark Messenger Playable Demo" build (Dec 2003), the second source.
# The converted files are committed, so this only needs to run when the
# conversion itself changes.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${1:-$ROOT/../Dark MessengerFrozen/Dark Messenger}"
DEMO="${2:-$HOME/Downloads/Dark Messenger Playable Demo}"
OUT="$ROOT/assets"
[ -f "$SRC/Dark Messenger.dbpro" ] || { echo "original project not found at: $SRC" >&2; exit 1; }
[ -f "$DEMO/model/hand2/hand2.x" ] || { echo "playable demo not found at: $DEMO" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "ffmpeg is required" >&2; exit 1; }

echo "Building xconv..."
cargo build --release --quiet --manifest-path "$ROOT/tools/xconv/Cargo.toml"
XCONV="$ROOT/tools/xconv/target/release/xconv"

mkdir -p "$OUT/models" "$OUT/textures/lightmaps" "$OUT/sounds"

echo "Converting models..."
# Animation key times are in DarkBASIC "frames" (150 per key). The original
# advanced the death animation 2250 frames/s and the Darius statue 150 frames
# per game loop (~30 fps on 2003 hardware).
"$XCONV" "$SRC/level/one.x"              "$OUT/models/level.glb" --lightmap "$SRC/level/one_lm.x" --merge
"$XCONV" "$SRC/model/dariusanim.x"       "$OUT/models/darius.glb"       --double-sided --ticks-per-second 4500
"$XCONV" "$SRC/model/archer/archer1.x"   "$OUT/models/archer.glb"       --double-sided
"$XCONV" "$SRC/model/archer/archer2.x"   "$OUT/models/archer_death.glb" --double-sided --ticks-per-second 2250
"$XCONV" "$SRC/model/arrow.x"            "$OUT/models/arrow.glb"        --double-sided --search "$SRC/level"
# First-person view models, as loaded by the demo (model\hand2, model\dagger2).
"$XCONV" "$DEMO/model/hand2/hand2.x"     "$OUT/models/hands.glb"        --double-sided
"$XCONV" "$DEMO/model/dagger2/dagger2.x" "$OUT/models/dagger.glb"       --double-sided

echo "Converting textures..."
img() { ffmpeg -loglevel error -y -i "$1" "$2"; }
for f in "$SRC"/level/one*.bmp; do
    img "$f" "$OUT/textures/lightmaps/$(basename "${f%.bmp}").png"
done
for i in 1 2 3; do
    img "$SRC/l$i.bmp" "$OUT/textures/lightning$i.png"
done
cp "$SRC/sky1.jpg" "$OUT/textures/sky.jpg"

echo "Converting sounds..."
ffmpeg -loglevel error -y -i "$SRC/Blowing wind.wav" -c:a libvorbis -q:a 3 "$OUT/sounds/wind.ogg"
# Sound effects are normalized to 16-bit PCM (chant.wav was 8-bit).
sfx() { ffmpeg -loglevel error -y -i "$1" -c:a pcm_s16le "$2"; }
sfx "$SRC/CustomLightning1.wav" "$OUT/sounds/lightning.wav"
sfx "$SRC/Chanttone.wav"        "$OUT/sounds/chant.wav"
sfx "$SRC/arrow.wav"            "$OUT/sounds/arrow.wav"
# Dagger sounds from the playable demo.
sfx "$DEMO/Whoosh.wav"          "$OUT/sounds/dagger_throw.wav"
sfx "$DEMO/knife.wav"           "$OUT/sounds/dagger_hit.wav"
sfx "$DEMO/grunt.wav"           "$OUT/sounds/archer_grunt.wav"

du -sh "$OUT"/*
