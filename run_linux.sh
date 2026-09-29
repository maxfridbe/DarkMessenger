#!/bin/bash
# Build and run natively on Linux. Pass "debug" for a debug build.

MODE="release"
FLAG="--release"

if [ "$1" == "debug" ]; then
    MODE="debug"
    FLAG=""
fi

echo "Building and running natively on Linux ($MODE)..."
cargo run $FLAG
