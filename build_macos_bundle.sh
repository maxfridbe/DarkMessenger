#!/bin/bash
# Runs INSIDE the Containerfile.macos image (see build_macos.sh, which is the
# entry point you actually invoke). Cross-compiles the game for Apple Silicon,
# wraps it in a .app bundle, ad-hoc signs it and packages a .dmg.
#
# Everything lands in target/macos_dist/.
cd "$(dirname "$0")"
source ./game.env
set -euo pipefail

VERSION="$(cat version.txt)"
TARGET="aarch64-apple-darwin"
DIST_DIR="target/macos_dist"
APP="$DIST_DIR/$GAME_LABEL.app"
DMG="$DIST_DIR/$GAME_NAME-macos-arm64-v$VERSION.dmg"

if [ -z "${MACOS_SDK_PATH:-}" ] || [ ! -d "${MACOS_SDK_PATH:-/nonexistent}" ]; then
    echo "ERROR: no macOS SDK in this environment." >&2
    echo "This script is meant to run inside the Containerfile.macos image." >&2
    echo "Use ./build_macos.sh from the host instead." >&2
    exit 1
fi

echo "=== Cross-compiling $GAME_NAME $VERSION for $TARGET ==="
echo "    SDK:    $MACOS_SDK_PATH"
echo "    min OS: ${MACOSX_DEPLOYMENT_TARGET:-?}"
# --bin: the lib target (staticlib/cdylib, for Android) has nothing to do with
# the desktop build and only slows this down.
cargo build --release --target "$TARGET" --bin "$GAME_NAME"

BIN="target/$TARGET/release/$GAME_NAME"

echo "=== Assembling $APP ==="
rm -rf "$DIST_DIR"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

install -m0755 "$BIN" "$APP/Contents/MacOS/$GAME_NAME"
"${MACOS_CROSS_TRIPLE}-strip" -S -x "$APP/Contents/MacOS/$GAME_NAME" 2>/dev/null || true

# Assets are embedded in the binary via bevy_embedded_assets, but shipping the
# folder too keeps hot-swapping possible (same as the Windows/Linux packages).
cp -r assets "$APP/Contents/Resources/"

# Icon: drop a 1024x1024 assets/macos-icon.png in for a proper one. The Android
# launcher icon is only 48x48, so it is a low-res fallback, not a good icon.
ICON_SRC=""
for candidate in assets/macos-icon.png assets/android-res/mipmap-mdpi/ic_launcher.png; do
    [ -f "$candidate" ] && { ICON_SRC="$candidate"; break; }
done
ICON_KEY=""
if [ -n "$ICON_SRC" ] && command -v png2icns >/dev/null 2>&1; then
    if png2icns "$APP/Contents/Resources/AppIcon.icns" "$ICON_SRC" >/dev/null 2>&1; then
        ICON_KEY=$'\n    <key>CFBundleIconFile</key>\n    <string>AppIcon</string>'
        echo "    icon:   $ICON_SRC -> AppIcon.icns"
    else
        echo "    icon:   png2icns rejected $ICON_SRC (needs 16/32/48/128/256/512/1024 px) - skipping"
    fi
else
    echo "    icon:   none (add assets/macos-icon.png at 1024x1024)"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>$GAME_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$MACOS_BUNDLE_ID</string>$ICON_KEY
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>$GAME_LABEL</string>
    <key>CFBundleDisplayName</key>
    <string>$GAME_LABEL</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>${MACOSX_DEPLOYMENT_TARGET:-11.0}</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key>
    <true/>
</dict>
</plist>
PLIST

# Apple Silicon refuses to exec an arm64 binary with no code signature at all,
# so the ad-hoc signature here is required for the app to launch - it is not
# the same thing as Developer ID signing/notarization (see README).
echo "=== Ad-hoc signing ==="
rcodesign sign "$APP" 2>&1 | sed 's/^/    /'

echo "=== Building $DMG ==="
STAGE="$DIST_DIR/.dmg-stage"
mkdir -p "$STAGE"
cp -a "$APP" "$STAGE/"
# Drag-to-install target, the convention every macOS user expects.
ln -s /Applications "$STAGE/Applications"

# xorrisofs writes an ISO9660+Rock Ridge image; macOS mounts that as a
# read-only .dmg. Rock Ridge is what carries the executable bit on the binary
# and the /Applications symlink through. (A real compressed UDBZ/HFS+ image
# needs Apple's hdiutil or a from-source libdmg-hfsplus.)
# xorriso prints a version banner on every run - only surface it on failure.
LOG="$(mktemp)"
if ! xorrisofs -quiet -D -l -r -no-pad -V "$GAME_LABEL" -o "$DMG" "$STAGE" >"$LOG" 2>&1; then
    cat "$LOG" >&2; rm -f "$LOG"; exit 1
fi
rm -f "$LOG"
rm -rf "$STAGE"

echo ""
echo "Done!"
echo "  App: $APP"
echo "  DMG: $DMG ($(du -h "$DMG" | cut -f1))"
file "$APP/Contents/MacOS/$GAME_NAME"
