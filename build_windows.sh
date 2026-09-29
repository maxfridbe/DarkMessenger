#!/bin/bash
# Cross-compile a Windows build from Linux using the MinGW-w64 GNU toolchain,
# then package exe + assets into a folder you can zip and ship.
source "$(dirname "$0")/game.env"

echo "=== Preparing Windows Build Environment ==="
rustup target add x86_64-pc-windows-gnu

echo "=== Building for Windows (Release) ==="
cargo build --release --target x86_64-pc-windows-gnu

if [ $? -eq 0 ]; then
    echo "=== Build Successful! ==="

    DIST_DIR="target/windows_dist"
    echo "Packaging into $DIST_DIR..."

    mkdir -p "$DIST_DIR"

    # Copy the Windows executable
    cp "target/x86_64-pc-windows-gnu/release/$GAME_NAME.exe" "$DIST_DIR/"

    # Copy the assets folder (assets are also embedded in the exe via
    # bevy_embedded_assets, but shipping the folder keeps hot-swapping easy)
    cp -r assets "$DIST_DIR/"

    echo "Done! The Windows build is ready."
    echo "Folder: $DIST_DIR"
    echo "You can zip this folder and send it to a Windows machine to play."
else
    echo "=== Build Failed ==="
    echo "You may need to install the MinGW-w64 C/C++ compiler for cross-compiling."
    echo "Try one of the following depending on your Linux distribution:"
    echo "  Ubuntu/Debian: sudo apt install gcc-mingw-w64"
    echo "  Fedora:        sudo dnf install mingw64-gcc"
    echo "  Arch:          sudo pacman -S mingw-w64-gcc"
fi
