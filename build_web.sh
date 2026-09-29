#!/bin/bash
# Build the browser (WASM) version: compile to wasm32-unknown-unknown, run
# wasm-bindgen to generate the JS glue, and package a static site folder.
# Assets are embedded in the .wasm via bevy_embedded_assets, so the output
# is fully self-contained and works from any static file host.
cd "$(dirname "$0")"
source ./game.env
set -e

echo "=== Preparing WASM Build Environment ==="
rustup target add wasm32-unknown-unknown

echo "=== Building for wasm32-unknown-unknown (wasm-release profile) ==="
# getrandom needs to be told to use the browser's crypto API (see Cargo.toml)
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }--cfg getrandom_backend=\"wasm_js\""
# --bin: without it the bin and cdylib lib targets both emit
# $GAME_NAME.wasm into the same directory and whichever links last wins.
cargo build --profile wasm-release --target wasm32-unknown-unknown --bin "$GAME_NAME"

# wasm-bindgen-cli MUST match the wasm-bindgen crate version in Cargo.lock.
WBV=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | grep '^version' | cut -d'"' -f2)
if ! command -v wasm-bindgen &>/dev/null || [ "$(wasm-bindgen --version | awk '{print $2}')" != "$WBV" ]; then
    echo "Installing wasm-bindgen-cli $WBV (must match the wasm-bindgen crate)..."
    cargo install wasm-bindgen-cli --version "$WBV" --locked --force
fi

DIST_DIR="target/web_dist"
echo "Packaging into $DIST_DIR..."
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

wasm-bindgen --no-typescript --target web \
    --out-dir "$DIST_DIR" --out-name "$GAME_NAME" \
    "target/wasm32-unknown-unknown/wasm-release/$GAME_NAME.wasm"

# Shrink further with binaryen's wasm-opt if available (~30-50% smaller).
# Needs binaryen >= 116: older versions (e.g. Ubuntu's apt v108) corrupt the
# wasm-bindgen externref table export and the game dies on startup with
# "WebAssembly.Table.grow(): failed to grow table".
WASM="$DIST_DIR/${GAME_NAME}_bg.wasm"
if command -v wasm-opt &>/dev/null; then
    WOV=$(wasm-opt --version | grep -oE '[0-9]+' | head -1)
    if [ "${WOV:-0}" -ge 116 ]; then
        BEFORE=$(ls -lh "$WASM" | awk '{print $5}')
        echo "Running wasm-opt -Oz ($BEFORE before)..."
        wasm-opt -Oz --output "$WASM.opt" "$WASM" && mv "$WASM.opt" "$WASM"
    else
        echo "NOTE: wasm-opt $WOV is too old (< 116) and would break the build - skipping extra shrink"
    fi
else
    echo "NOTE: wasm-opt not found (apt/dnf: binaryen) - skipping extra shrink"
fi

sed "s/{{GAME_NAME}}/$GAME_NAME/g" web/index.html > "$DIST_DIR/index.html"

echo ""
echo "Done! The web build is ready in $DIST_DIR"
echo "  Size: $(ls -lh "$DIST_DIR/${GAME_NAME}_bg.wasm" | awk '{print $5}') (wasm)"
echo "Try it locally:  python3 -m http.server -d $DIST_DIR 8080"
echo "Then open:       http://localhost:8080"
