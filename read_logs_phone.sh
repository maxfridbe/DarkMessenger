#!/bin/bash
# Dump the latest physical-phone logs filtered for Bevy/Rust/panic output.
source "$(dirname "$0")/game.env"

adb -d logcat -d | grep -iE "bevy|rust|panic|$ANDROID_PACKAGE" | tail -n 50
