#!/bin/bash
# Build the macOS arm64 (Apple Silicon) .app + .dmg from Linux, inside a
# podman container. No Mac required.
#
#   ./build_macos.sh                 build (reuses the cached image)
#   ./build_macos.sh --rebuild       force the cross-toolchain image to rebuild
#   ./build_macos.sh --shell         drop into the container for debugging
#
# The first run builds Containerfile.macos, which compiles osxcross from
# source and downloads Apple's macOS SDK from a public mirror. That takes
# ~10-20 minutes and a few GB; every run after that reuses the image.
#
# Output: target/macos_dist/<Game>.app and target/macos_dist/<game>-macos-arm64-v<version>.dmg
cd "$(dirname "$0")"
source ./game.env
set -euo pipefail

# Shared with GameBase: the image is a generic osxcross toolchain.
IMAGE="gamebase-macos-builder"
# Bumping the SDK version changes the toolchain, so it is part of the tag -
# otherwise a stale image silently keeps being reused. Note that overriding
# this alone is not enough: Containerfile.macos pins the SDK's sha256 too, and
# both have to move together.
SDK_VERSION="${MACOS_SDK_VERSION:-14.5}"
TAG="$IMAGE:$SDK_VERSION"
CARGO_VOLUME="gamebase-macos-cargo-registry"

ENGINE="${CONTAINER_ENGINE:-}"
if [ -z "$ENGINE" ]; then
    if command -v podman >/dev/null 2>&1; then ENGINE=podman
    elif command -v docker >/dev/null 2>&1; then ENGINE=docker
    else
        echo "ERROR: neither podman nor docker found." >&2
        echo "  Fedora: sudo dnf install podman   Debian/Ubuntu: sudo apt install podman" >&2
        exit 1
    fi
fi

REBUILD=0
CMD=(./build_macos_bundle.sh)
for arg in "$@"; do
    case "$arg" in
        --rebuild) REBUILD=1 ;;
        --shell)   CMD=(bash) ;;
        *) echo "Unknown option: $arg" >&2; exit 1 ;;
    esac
done

# `image inspect` rather than podman's `image exists`, so docker works too.
if [ "$REBUILD" = 1 ] || ! "$ENGINE" image inspect "$TAG" >/dev/null 2>&1; then
    echo "=== Building cross-toolchain image $TAG (this takes a while the first time) ==="
    "$ENGINE" build -f Containerfile.macos -t "$TAG" \
        --build-arg "MACOS_SDK_VERSION=$SDK_VERSION" .
fi

# label=disable instead of :z - relabelling the whole repo (target/ included)
# on every run is slow, and this container only ever touches the bind mount.
RUN_ARGS=(--rm --security-opt label=disable -v "$PWD:/work")
# The crate registry lives in a named volume so repeat builds skip the
# downloads. CI sets MACOS_CARGO_REGISTRY to a plain directory instead, so
# actions/cache can persist it between runs.
if [ -n "${MACOS_CARGO_REGISTRY:-}" ]; then
    mkdir -p "$MACOS_CARGO_REGISTRY"
    RUN_ARGS+=(-v "$(realpath "$MACOS_CARGO_REGISTRY"):/usr/local/cargo/registry")
else
    RUN_ARGS+=(-v "$CARGO_VOLUME:/usr/local/cargo/registry")
fi
# Rootless podman already maps container root to the invoking user, so files
# come out owned correctly. Docker runs as real root and would leave root-owned
# build output in the repo, so pin the uid there.
if [ "$ENGINE" = docker ]; then
    RUN_ARGS+=(--user "$(id -u):$(id -g)")
fi
# --shell wants stdin; only ask for a TTY when there actually is one, so it
# still works with a piped-in script.
if [ "${CMD[0]}" = bash ]; then
    RUN_ARGS+=(-i)
    [ -t 0 ] && RUN_ARGS+=(-t)
fi

echo "=== Running macOS build in $ENGINE ==="
"$ENGINE" run "${RUN_ARGS[@]}" "$TAG" "${CMD[@]}"
