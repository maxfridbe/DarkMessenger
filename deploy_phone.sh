#!/bin/bash
# Install the cargo-apk ARM64 APK onto a USB-connected phone, launch it,
# and follow logcat. Prefers release, falls back to debug.
source "$(dirname "$0")/game.env"

# Check for release first, then debug
if [ -f "target/release/apk/${GAME_NAME}_phone.apk" ]; then
    APK_PATH="target/release/apk/${GAME_NAME}_phone.apk"
    MODE="RELEASE"
elif [ -f "target/debug/apk/${GAME_NAME}_phone.apk" ]; then
    APK_PATH="target/debug/apk/${GAME_NAME}_phone.apk"
    MODE="DEBUG"
else
    echo "Error: Phone APK not found."
    echo "Please run ./build_cargo_apk.sh or ./build_cargo_apk_debug.sh first."
    exit 1
fi

# Get human-readable file age so you know you're not installing a stale build
if [[ "$OSTYPE" == "darwin"* ]]; then
    FILE_EPOCH=$(stat -f "%m" "$APK_PATH")
else
    FILE_EPOCH=$(stat -c %Y "$APK_PATH")
fi

CURRENT_EPOCH=$(date +%s)
AGE_SECONDS=$((CURRENT_EPOCH - FILE_EPOCH))

if [ $AGE_SECONDS -lt 60 ]; then
    AGE_STR="$AGE_SECONDS seconds ago"
elif [ $AGE_SECONDS -lt 3600 ]; then
    AGE_STR="$((AGE_SECONDS / 60)) minutes ago"
else
    AGE_STR="$((AGE_SECONDS / 3600)) hours ago"
fi

echo "=== Deploying APK ==="
echo "Path: $APK_PATH"
echo "Mode: $MODE"
echo "Created: $AGE_STR"
echo "====================="

echo "=== Installing to Physical Phone ==="
adb -d install -r "$APK_PATH"

if [ $? -ne 0 ]; then
    echo "Installation failed. Is your phone plugged in and USB debugging enabled?"
    exit 1
fi

echo "=== Clearing old logs ==="
adb -d logcat -c

echo "=== Starting Application ==="
adb -d shell am start -n "$ANDROID_PACKAGE/android.app.NativeActivity"

echo "=== Following logs (Press Ctrl+C to stop) ==="
adb -d logcat *:S Rust:V AndroidRuntime:E DEBUG:E $ANDROID_PACKAGE:V
