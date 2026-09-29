#!/bin/bash
# Install the cargo-apk emulator APK onto the running emulator, launch it,
# and follow logcat. Prefers release, falls back to debug.
source "$(dirname "$0")/game.env"

# Check for release first, then debug
if [ -f "target/release/apk/${GAME_NAME}_emulator.apk" ]; then
    APK_PATH="target/release/apk/${GAME_NAME}_emulator.apk"
    MODE="RELEASE"
elif [ -f "target/debug/apk/${GAME_NAME}_emulator.apk" ]; then
    APK_PATH="target/debug/apk/${GAME_NAME}_emulator.apk"
    MODE="DEBUG"
else
    echo "Error: Emulator APK not found."
    echo "Please run ./build_cargo_apk.sh or ./build_cargo_apk_debug.sh first."
    exit 1
fi

echo "=== Installing to Emulator ($MODE) ==="
adb -e install -r "$APK_PATH"

echo "=== Clearing old logs ==="
adb -e logcat -c

echo "=== Starting Application ==="
adb -e shell am start -n "$ANDROID_PACKAGE/android.app.NativeActivity"

echo "=== Following logs (Press Ctrl+C to stop) ==="
adb -e logcat *:S Rust:V AndroidRuntime:E DEBUG:E $ANDROID_PACKAGE:V
