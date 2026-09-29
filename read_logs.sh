#!/bin/bash
# Dump the latest emulator logs filtered for Bevy/Rust/panic output.
source "$(dirname "$0")/game.env"

adb -e logcat -d | grep -iE "bevy|rust|panic|$ANDROID_PACKAGE" | tail -n 50
