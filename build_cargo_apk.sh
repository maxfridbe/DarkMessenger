#!/bin/bash
# Android path A (primary): cargo-apk. Builds two release APKs from the
# cdylib — one for the x86_64 emulator, one for ARM64 phones.
source "$(dirname "$0")/game.env"

BUILD_MODE="release"
CARGO_FLAG="--release"

echo "Building Emulator APK (x86_64) [$BUILD_MODE]..."
cargo apk build $CARGO_FLAG --lib --target x86_64-linux-android
mv target/$BUILD_MODE/apk/$GAME_NAME.apk target/$BUILD_MODE/apk/${GAME_NAME}_emulator.apk

echo "Building Phone APK (ARM64) [$BUILD_MODE]..."
cargo apk build $CARGO_FLAG --lib --target aarch64-linux-android
mv target/$BUILD_MODE/apk/$GAME_NAME.apk target/$BUILD_MODE/apk/${GAME_NAME}_phone.apk

echo ""
echo "Done! Generated two $BUILD_MODE APKs:"
echo "1. target/$BUILD_MODE/apk/${GAME_NAME}_emulator.apk (For Emulator)"
echo "   Size: $(ls -lh target/$BUILD_MODE/apk/${GAME_NAME}_emulator.apk | awk '{print $5}')"
echo "2. target/$BUILD_MODE/apk/${GAME_NAME}_phone.apk (For a physical phone)"
echo "   Size: $(ls -lh target/$BUILD_MODE/apk/${GAME_NAME}_phone.apk | awk '{print $5}')"
