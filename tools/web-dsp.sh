#!/usr/bin/env bash
# Copyright (C) 2026 ItsAlphaNeon
# SPDX-License-Identifier: GPL-3.0-or-later
# This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

set -euo pipefail
cd "$(dirname "$0")/.."
: "${EMSDK:?Set EMSDK to the Emscripten 4.0.23 SDK directory}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
export CARGO_PROFILE_DEV_DEBUG=0
output="${DSP_BUILD_DIR:-$CARGO_TARGET_DIR/web-dsp}"
mkdir -p "$output"
cargo run -p os-dsp --example stretch_probe --locked -- "$output/native.f32"
RUSTFLAGS="-C opt-level=3 -C target-feature=+simd128" cargo build -p os-dsp --example stretch_probe --target wasm32-unknown-unknown --locked
tools/web-bindgen.sh "$CARGO_TARGET_DIR/wasm32-unknown-unknown/debug/examples/stretch_probe.wasm" "$output"
printf 'DSP fixture: %s\n' "$output"
