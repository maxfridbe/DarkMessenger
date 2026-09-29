#!/bin/bash
# Start a clean Android emulator with Swangle GPU emulation (most stable
# for Bevy/wgpu Vulkan rendering), then wait until it has fully booted.
source "$(dirname "$0")/game.env"

AVD_NAME="bevy_test_2"

# Kill existing emulators to ensure a clean state
echo "Cleaning up existing emulators..."
pkill -x emulator || true
pkill -x qemu-system-x86_64 || true
pkill -x qemu-system-x86_64-headless || true
sleep 2

echo "Starting Emulator ($AVD_NAME with Swangle)..."
# Swangle is generally more capable and stable for modern Vulkan emulation
emulator -avd "$AVD_NAME" -no-snapshot -memory 2048 -gpu swangle_indirect -wipe-data -no-audio -no-metrics &

echo "Waiting for emulator to connect to adb..."
sleep 5
adb wait-for-device

echo "Waiting for boot completion (this may take a minute)..."
while [ "`adb shell getprop sys.boot_completed | tr -d '\r'`" != "1" ] ; do
    echo -n "."
    sleep 5
done
echo ""
echo "Emulator is ready!"
