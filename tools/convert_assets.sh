#!/bin/bash
# Regenerates assets/ from the original DarkBASIC Pro project (final build,
# 18 Nov 2003 — "Dark Messenger Source and Models").
#
#   tools/convert_assets.sh [path/to/original project]
#
# Models (.x) go through tools/xconv into .glb (animations keep DarkBASIC's
# 150-ticks-per-frame timeline at 4800 ticks/s; the game drives frames the
# way `set object frame` did). Images become .png, the wind and intro tracks
# .ogg, sound effects 16-bit .wav. The converted files are committed, so this
# only needs to run when the conversion itself changes.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SRC="${1:-$ROOT/../Dark MessengerNov2003}"
OUT="$ROOT/assets"
[ -f "$SRC/Dark Messenger.dbpro" ] || { echo "original project not found at: $SRC" >&2; exit 1; }
[ -f "$SRC/weapon.dba" ] || { echo "$SRC is not the final (Nov 2003) build" >&2; exit 1; }
command -v ffmpeg >/dev/null || { echo "ffmpeg is required" >&2; exit 1; }

echo "Building xconv..."
cargo build --release --quiet --manifest-path "$ROOT/tools/xconv/Cargo.toml"
XCONV="$ROOT/tools/xconv/target/release/xconv"

rm -rf "$OUT/models" "$OUT/textures" "$OUT/sounds" "$OUT/ui"
mkdir -p "$OUT/models" "$OUT/textures/lightmaps" "$OUT/textures/fire" "$OUT/textures/water" \
    "$OUT/textures/vortex" "$OUT/sounds" "$OUT/ui"

echo "Converting models..."
x() { "$XCONV" "$@" | sed "s|$SRC/||; s|$OUT/||"; }
x "$SRC/level/one.x"                 "$OUT/models/level1.glb" --lightmap "$SRC/level/one_lm.x" --merge
x "$SRC/level/two.x"                 "$OUT/models/level2.glb" --lightmap "$SRC/level/two_lm.x" --merge
x "$SRC/model/knight2/knight2.x"     "$OUT/models/knight.glb" --double-sided
x "$SRC/model/archer3/archer3.x"     "$OUT/models/archer.glb" --double-sided
x "$SRC/model/arrow.x"               "$OUT/models/arrow.glb"  --double-sided --search "$SRC/level"
x "$SRC/model/hand2/hand2.x"         "$OUT/models/hands.glb"  --double-sided
x "$SRC/model/dagger2/dagger2.x"     "$OUT/models/dagger.glb" --double-sided
x "$SRC/model/spike/spike.x"         "$OUT/models/spike.glb"  --double-sided
x "$SRC/model/book/book.x"           "$OUT/models/book.glb"   --double-sided
x "$SRC/model/d2/d2.x"               "$OUT/models/darius.glb" --double-sided

echo "Converting textures..."
img() { ffmpeg -loglevel error -y -i "$1" "${@:3}" "$2"; }
for f in "$SRC"/level/one*.bmp "$SRC"/level/two*.bmp; do
    img "$f" "$OUT/textures/lightmaps/$(basename "${f%.bmp}").png"
done
for i in 1 2 3; do img "$SRC/l$i.bmp" "$OUT/textures/lightning$i.png"; done
cp "$SRC/sky1.jpg" "$OUT/textures/sky.jpg"
for i in $(seq -w 1 50); do
    img "$SRC/image/fire/fire$i.bmp"      "$OUT/textures/fire/$i.png"
    img "$SRC/image/water/water ($i).bmp" "$OUT/textures/water/$i.png"
done
for i in $(seq 0 20); do img "$SRC/image/vortex/vortex ($i).bmp" "$OUT/textures/vortex/$(printf %02d "$i").png"; done
cp "$SRC/image/grafOne.png" "$OUT/textures/graffiti.png"

echo "Converting UI images..."
img "$SRC/image/menueFinal.bmp" "$OUT/ui/menu.png"
cp  "$SRC/image/load.jpg"       "$OUT/ui/loading.jpg"
# DarkBASIC sprites drew pure black as transparent.
img "$SRC/image/cross.bmp"      "$OUT/ui/crosshair.png" -vf "colorkey=black:0.08:0.1,format=rgba"
img "$SRC/health.bmp"           "$OUT/ui/health_fill.png"
img "$SRC/mana.bmp"             "$OUT/ui/mana_fill.png"
cp  "$SRC/healthbar.png"        "$OUT/ui/mana_frame.png"
cp  "$SRC/cast.png"             "$OUT/ui/health_frame.png"

echo "Converting sounds..."
ogg() { ffmpeg -loglevel error -y -i "$1" -c:a libvorbis -q:a 3 "$2"; }
ogg "$SRC/Blowing wind.wav" "$OUT/sounds/wind.ogg"
# The intro speech is quiet against the rest: normalise it to -11 LUFS
# with a narrow loudness range so the quiet lines come up.
ffmpeg -loglevel error -y -i "$SRC/darkness.wav" -af "loudnorm=I=-11:TP=-1:LRA=6" -ar 44100 \
    -c:a libvorbis -q:a 3 "$OUT/sounds/darkness.ogg"
sfx() { ffmpeg -loglevel error -y -i "$1" -c:a pcm_s16le "$2"; }
sfx "$SRC/CustomLightning1.wav" "$OUT/sounds/lightning.wav"
sfx "$SRC/Chanttone.wav"        "$OUT/sounds/chant.wav"
sfx "$SRC/arrow.wav"            "$OUT/sounds/arrow.wav"
sfx "$SRC/knife.wav"            "$OUT/sounds/knife.wav"
sfx "$SRC/Whoosh.wav"           "$OUT/sounds/throw.wav"
sfx "$SRC/scrm.wav"             "$OUT/sounds/scream.wav"
sfx "$SRC/boneSpike.wav"        "$OUT/sounds/bone.wav"
sfx "$SRC/grunt.wav"            "$OUT/sounds/grunt.wav"
sfx "$SRC/sword.wav"            "$OUT/sounds/sword.wav"
sfx "$SRC/yell2.wav"            "$OUT/sounds/yell.wav"

du -sh "$OUT"/*
