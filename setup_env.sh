#!/bin/bash
# Environment setup + verification for every build script in this repo.
#
#   ./setup_env.sh --check   -> only report what is present/missing (no sudo, no installs)
#   ./setup_env.sh           -> check everything, install/fix only what is missing
#
# Covers the requirements of:
#   run_linux.sh          (native toolchain + Bevy system libs)
#   build_windows.sh      (MinGW-w64 cross compiler + rust windows target)
#   build_macos.sh        (podman only - the cross toolchain is containerized)
#   build_cargo_apk*.sh   (cargo-apk, Android rust targets, SDK/NDK, keystore)
#   buildanddeploy.sh     (cargo-ndk, Java for Gradle, NDK libc++_shared.so)
#   deploy_*/run_emulator (adb, emulator)

set -u
cd "$(dirname "$0")"

CHECK_ONLY=0
[ "${1:-}" == "--check" ] && CHECK_ONLY=1

NDK_VERSION="26.1.10909125"
ANDROID_HOME="${ANDROID_HOME:-$HOME/android-sdk}"
KEYSTORE=$(grep -A1 'signing.release' Cargo.toml | grep 'path' | sed 's/.*= *"\(.*\)"/\1/')

MISSING=0
ok()   { printf "  [ OK ] %s\n" "$1"; }
bad()  { printf "  [MISS] %s\n" "$1"; MISSING=$((MISSING+1)); }
have() { command -v "$1" &>/dev/null; }

# Detect package manager (Debian/Ubuntu apt vs Fedora dnf)
if have apt-get; then PM="apt"; elif have dnf; then PM="dnf"; else PM="none"; fi

# ---------------------------------------------------------------------------
echo "=== 1. Native Linux build (run_linux.sh) ==="
have cc         && ok "C compiler (cc)"        || bad "C compiler        ($PM: build-essential / gcc gcc-c++)"
have pkg-config && ok "pkg-config"             || bad "pkg-config"
have cmake      && ok "cmake"                  || bad "cmake"

# Bevy's system libraries, checked via pkg-config: "pkgconfig-name|apt pkg|dnf pkg"
SYSLIBS="x11|libx11-dev|libX11-devel
alsa|libasound2-dev|alsa-lib-devel
libudev|libudev-dev|systemd-devel
wayland-client|libwayland-dev|wayland-devel
xkbcommon|libxkbcommon-dev|libxkbcommon-devel"
if have pkg-config; then
    while IFS='|' read -r lib aptpkg dnfpkg; do
        if pkg-config --exists "$lib"; then ok "lib: $lib"; else bad "lib: $lib   (apt: $aptpkg / dnf: $dnfpkg)"; fi
    done <<< "$SYSLIBS"
else
    bad "system libs unknown (pkg-config missing)"
fi

echo "=== 2. Rust toolchain ==="
have rustup && ok "rustup" || bad "rustup (https://rustup.rs)"
have cargo  && ok "cargo"  || bad "cargo"

echo "=== 3. Windows cross build (build_windows.sh) ==="
have x86_64-w64-mingw32-gcc && ok "MinGW-w64 gcc" || bad "MinGW-w64 gcc  (apt: gcc-mingw-w64-x86-64 / dnf: mingw64-gcc)"
if have rustup && rustup target list --installed 2>/dev/null | grep -q x86_64-pc-windows-gnu; then
    ok "rust target x86_64-pc-windows-gnu"
else
    bad "rust target x86_64-pc-windows-gnu"
fi

echo "=== 4. macOS arm64 cross build (build_macos.sh) ==="
# The whole toolchain lives in a container, so podman (or docker) is the only
# host requirement - nothing to install natively.
have podman && ok "podman" || { have docker && ok "docker (podman preferred)" || bad "podman  (apt: podman / dnf: podman) - needed by build_macos.sh"; }

echo "=== 5. Browser/WASM build (build_web.sh) ==="
if have rustup && rustup target list --installed 2>/dev/null | grep -q wasm32-unknown-unknown; then
    ok "rust target wasm32-unknown-unknown"
else
    bad "rust target wasm32-unknown-unknown"
fi
have wasm-bindgen && ok "wasm-bindgen-cli" || bad "wasm-bindgen-cli (build_web.sh auto-installs the version matching Cargo.lock)"
have wasm-opt && ok "wasm-opt (binaryen)" || bad "wasm-opt  (apt/dnf: binaryen — optional, shrinks the .wasm ~30-50%)"

echo "=== 6. Android APK builds (build_cargo_apk*.sh / buildanddeploy.sh) ==="
for t in aarch64-linux-android x86_64-linux-android; do
    if have rustup && rustup target list --installed 2>/dev/null | grep -q "$t"; then
        ok "rust target $t"
    else
        bad "rust target $t"
    fi
done
have cargo-apk && ok "cargo-apk" || bad "cargo-apk (cargo install cargo-apk)"
have cargo-ndk && ok "cargo-ndk" || bad "cargo-ndk (cargo install cargo-ndk)"
have java      && ok "java (Gradle path)" || bad "java  (apt: openjdk-21-jdk-headless / dnf: java-21-openjdk-headless)"
have wget      && ok "wget"  || bad "wget"
have unzip     && ok "unzip" || bad "unzip"

[ -d "$ANDROID_HOME" ]                          && ok "Android SDK dir ($ANDROID_HOME)" || bad "Android SDK dir ($ANDROID_HOME)"
[ -x "$ANDROID_HOME/platform-tools/adb" ]       && ok "adb (platform-tools)"            || bad "adb (sdkmanager platform-tools)"
[ -x "$ANDROID_HOME/emulator/emulator" ]        && ok "emulator"                        || bad "emulator (sdkmanager emulator + system image)"
[ -d "$ANDROID_HOME/ndk/$NDK_VERSION" ]         && ok "NDK $NDK_VERSION"                || bad "NDK $NDK_VERSION"
[ -n "$KEYSTORE" ] && [ -f "$KEYSTORE" ]        && ok "release keystore ($KEYSTORE)"    || bad "release keystore ($KEYSTORE) — Cargo.toml [signing.release]"

echo ""
if [ "$MISSING" -eq 0 ]; then
    echo "All checks passed. Every build script's requirements are satisfied."
    exit 0
fi
echo "$MISSING requirement(s) missing."
if [ "$CHECK_ONLY" -eq 1 ]; then
    echo "Run ./setup_env.sh (without --check) to install the missing pieces."
    exit 1
fi

# ---------------------------------------------------------------------------
echo ""
echo "=== Installing missing requirements ==="
set -e

if [ "$PM" == "apt" ]; then
    sudo apt-get update
    sudo apt-get install -y \
        build-essential pkg-config cmake curl wget unzip \
        libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev \
        openjdk-21-jdk-headless \
        gcc-mingw-w64-x86-64 g++-mingw-w64-x86-64 \
        podman \
        binaryen
elif [ "$PM" == "dnf" ]; then
    sudo dnf install -y \
        gcc gcc-c++ pkg-config cmake curl wget unzip \
        libX11-devel alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel \
        java-21-openjdk-headless \
        mingw64-gcc mingw64-gcc-c++ \
        podman \
        binaryen
else
    echo "No supported package manager (apt/dnf) found - install system packages manually."
fi

if ! have rustup; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
fi

echo "Adding cross-compile targets to Rust..."
rustup target add x86_64-pc-windows-gnu
rustup target add wasm32-unknown-unknown
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android

have cargo-apk || cargo install cargo-apk
have cargo-ndk || cargo install cargo-ndk
# build_web.sh re-installs the exact version matching Cargo.lock if needed
have wasm-bindgen || cargo install wasm-bindgen-cli

# Android SDK/NDK Setup
if [ ! -d "$ANDROID_HOME/ndk/$NDK_VERSION" ] || [ ! -x "$ANDROID_HOME/platform-tools/adb" ]; then
    echo "Setting up Android SDK in $ANDROID_HOME..."
    if [ ! -d "$ANDROID_HOME/cmdline-tools/latest" ]; then
        mkdir -p "$ANDROID_HOME/cmdline-tools"
        wget https://dl.google.com/android/repository/commandlinetools-linux-11076708_latest.zip -O /tmp/tools.zip
        unzip -q /tmp/tools.zip -d "$ANDROID_HOME/cmdline-tools"
        mv "$ANDROID_HOME/cmdline-tools/cmdline-tools" "$ANDROID_HOME/cmdline-tools/latest"
        rm /tmp/tools.zip
    fi
    export PATH="$PATH:$ANDROID_HOME/cmdline-tools/latest/bin"
    yes | sdkmanager --licenses --sdk_root="$ANDROID_HOME"
    sdkmanager --sdk_root="$ANDROID_HOME" "platform-tools" "platforms;android-34" "platforms;android-30" "ndk;$NDK_VERSION" "build-tools;34.0.0" "emulator" "system-images;android-30;google_apis;x86_64"
fi

# Debug keystore for APK signing (matches Cargo.toml [signing.release])
if [ -n "$KEYSTORE" ] && [ ! -f "$KEYSTORE" ] && have keytool; then
    echo "Generating debug keystore at $KEYSTORE..."
    mkdir -p "$(dirname "$KEYSTORE")"
    keytool -genkey -v -keystore "$KEYSTORE" -storepass android -alias androiddebugkey \
        -keypass android -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
fi

# Add to .bashrc if not present
if ! grep -q "ANDROID_HOME" ~/.bashrc; then
    echo "Adding Android environment variables to .bashrc..."
    echo "export ANDROID_HOME=$ANDROID_HOME" >> ~/.bashrc
    echo "export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/$NDK_VERSION" >> ~/.bashrc
    echo "export PATH=\$PATH:\$ANDROID_HOME/platform-tools:\$ANDROID_HOME/cmdline-tools/latest/bin" >> ~/.bashrc
fi

echo ""
echo "=== Re-checking ==="
exec "$0" --check
