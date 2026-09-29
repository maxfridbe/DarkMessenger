#!/bin/bash
# Android path B (alternative): cargo-ndk + Gradle. Builds the Rust cdylib
# for both ABIs into jniLibs, then lets Gradle produce a universal APK with
# a Java GameActivity wrapper. Use this path when you need Java/Kotlin
# interop, Play Store bundles, or GameActivity features.
source "$(dirname "$0")/game.env"

APK_PATH="app/build/outputs/apk/debug/app-debug.apk"

echo "=== Step 1: Building Rust Library ==="
cargo ndk -t arm64-v8a -t x86_64 -o app/src/main/jniLibs build --package $GAME_NAME

echo "=== Step 2: Copying required C++ standard library ==="
mkdir -p app/src/main/jniLibs/arm64-v8a
mkdir -p app/src/main/jniLibs/x86_64
cp $ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so app/src/main/jniLibs/arm64-v8a/
cp $ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/sysroot/usr/lib/x86_64-linux-android/libc++_shared.so app/src/main/jniLibs/x86_64/

echo "=== Step 3: Building APK ==="
./gradlew assembleDebug

echo ""
echo "=========================================================="
echo "=== BUILD COMPLETE ==="
echo "=========================================================="
echo "The generated APK is located at:"
echo "-> $(pwd)/$APK_PATH"
echo "   Size: $(ls -lh "$APK_PATH" | awk '{print $5}')"
echo ""
echo "This is a universal (fat) APK containing both architectures:"
echo "  1. arm64-v8a : modern physical Android devices"
echo "  2. x86_64    : desktop Android emulators"
echo "=========================================================="
echo ""

echo "=== Step 4: Installing to Emulator ==="
# -e targets the currently running emulator specifically
adb -e install -r "$APK_PATH"

echo "=== Step 5: Clearing old logs ==="
adb -e logcat -c

echo "=== Step 6: Starting Application ==="
adb -e shell am start -n "$GRADLE_PACKAGE/.MainActivity"

echo "=== Step 7: Following logs (Press Ctrl+C to stop) ==="
adb -e logcat *:S Rust:V AndroidRuntime:E DEBUG:E $GRADLE_PACKAGE:V
