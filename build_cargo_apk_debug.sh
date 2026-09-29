#!/bin/bash
# Android path A (primary): cargo-apk, debug variant. Faster builds, larger APK.
source "$(dirname "$0")/game.env"

BUILD_MODE="debug"
CARGO_FLAG=""

echo "Building Emulator APK (x86_64) [$BUILD_MODE]..."
cargo apk build $CARGO_FLAG --lib --target x86_64-linux-android
mv target/$BUILD_MODE/apk/$GAME_NAME.apk target/$BUILD_MODE/apk/${GAME_NAME}_emulator.apk

echo "Building Phone APK (ARM64) [$BUILD_MODE]..."
cargo apk build $CARGO_FLAG --lib --target aarch64-linux-android
mv target/$BUILD_MODE/apk/$GAME_NAME.apk target/$BUILD_MODE/apk/${GAME_NAME}_phone.apk

echo ""
echo "Done! Generated two $BUILD_MODE APKs:"
echo "1. target/$BUILD_MODE/apk/${GAME_NAME}_emulator.apk (For Emulator)"
echo "2. target/$BUILD_MODE/apk/${GAME_NAME}_phone.apk (For a physical phone)"
