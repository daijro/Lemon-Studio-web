#!/usr/bin/env bash
# Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
# and not any other part of Lemon Studio.

set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
export DSP_BUILD_DIR="${DSP_BUILD_DIR:-$CARGO_TARGET_DIR/web-dsp}"
export WEB_FILE_FIXTURE="${WEB_FILE_FIXTURE:-$PWD/target/web-files}"
web_tools="${WEB_TOOLS_DIR:-$CARGO_TARGET_DIR/web-tools}"
export EMSDK="${EMSDK:-$web_tools/emsdk}"
export WASM_BINDGEN="${WASM_BINDGEN:-$web_tools/bin/wasm-bindgen}"
tools/web-dsp.sh
cargo run -p os-clap --example builtin_probe --locked -- "$DSP_BUILD_DIR/plugins-native.json"
tools/web-cargo.sh -p os-clap --example builtin_probe_web
tools/web-bindgen.sh "$CARGO_TARGET_DIR/wasm32-unknown-unknown/debug/examples/builtin_probe_web.wasm" "$DSP_BUILD_DIR"
cargo test -p os-app --lib --locked web_file_round_trip_and_invalid_import_preserve_project
