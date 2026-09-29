#!/bin/bash
# Install the Gradle-built universal APK onto the running emulator and
# follow logs (companion to buildanddeploy.sh when the APK already exists).
source "$(dirname "$0")/game.env"

APK_PATH="app/build/outputs/apk/debug/app-debug.apk"

echo "=== Installing to Emulator ==="
adb -e install -r "$APK_PATH"

echo "=== Clearing old logs ==="
adb -e logcat -c

echo "=== Starting Application ==="
adb -e shell am start -n "$GRADLE_PACKAGE/.MainActivity"

echo "=== Following logs (Press Ctrl+C to stop) ==="
adb -e logcat *:S Rust:V AndroidRuntime:E DEBUG:E $GRADLE_PACKAGE:V
