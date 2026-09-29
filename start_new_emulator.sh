#!/bin/bash
# Alternative emulator launcher using SwiftShader (pure-CPU GLES/Vulkan).
# Use this when the host GPU driver crashes the emulator with wgpu.
source "$(dirname "$0")/game.env"

AVD_NAME="bevy_swiftshader"

# Kill existing emulators to ensure a clean state
echo "Cleaning up existing emulators..."
pkill -x emulator || true
pkill -x qemu-system-x86_64 || true
pkill -x qemu-system-x86_64-headless || true
sleep 2

echo "Starting Emulator ($AVD_NAME with SwiftShader)..."
# Force system libraries to prevent library conflicts on Linux
export ANDROID_EMULATOR_USE_SYSTEM_LIBS=1
export QT_QPA_PLATFORM=xcb
export QT_X11_NO_MITSHM=1
unset WAYLAND_DISPLAY

emulator -avd "$AVD_NAME" -no-snapshot -memory 4096 -gpu guest -wipe-data -no-audio -no-metrics &

echo "Waiting for emulator to connect to adb..."
sleep 5
adb wait-for-device

echo "Waiting for boot completion (this may take a minute)..."
while [ "`adb shell getprop sys.boot_completed | tr -d '\r'`" != "1" ] ; do
    echo -n "."
    sleep 5
done
echo ""
echo "Emulator is ready! You can now run ./deploy_cargo_apk.sh"
