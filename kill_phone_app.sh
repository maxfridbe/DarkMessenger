#!/bin/bash
# Force-stop the game on a USB-connected phone.
source "$(dirname "$0")/game.env"

echo "=== Killing Application $ANDROID_PACKAGE on Physical Phone ==="
adb -d shell am force-stop "$ANDROID_PACKAGE"

if [ $? -eq 0 ]; then
    echo "Application successfully stopped."
else
    echo "Failed to stop application. Is your phone plugged in and USB debugging enabled?"
    exit 1
fi
