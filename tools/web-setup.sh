#!/usr/bin/env bash
# Copyright (C) 2026 ItsAlphaNeon
# SPDX-License-Identifier: GPL-3.0-or-later
# This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

set -euo pipefail
cd "$(dirname "$0")/.."
web_tools="${WEB_TOOLS_DIR:-${CARGO_TARGET_DIR:-$PWD/target}/web-tools}"
sdk="${EMSDK:-$web_tools/emsdk}"
cli="${WASM_BINDGEN:-$web_tools/bin/wasm-bindgen}"
rustup toolchain install nightly-2025-11-06 --profile minimal --component rust-src --target wasm32-unknown-unknown
if [[ ! -f "$sdk/emsdk" ]]; then
    # A build cache can bring back part of it (CI's target cache does), and git clone wants an empty folder.
    rm -rf "$sdk"
    mkdir -p "$(dirname "$sdk")"
    git clone https://github.com/emscripten-core/emsdk.git "$sdk"
    git -C "$sdk" checkout c0bb220cb6e6f4e0fabb6f6db9efd53390ef5e56
fi
"$sdk/emsdk" install 4.0.23
"$sdk/emsdk" activate 4.0.23
if [[ ! -x "$cli" ]] || [[ "$("$cli" --version)" != 'wasm-bindgen 0.2.129' ]]; then
    cargo install wasm-bindgen-cli --version 0.2.129 --locked --root "$web_tools"
fi
printf 'Web tooling ready. Run tools/web-build.sh with the same path environment.\n'
