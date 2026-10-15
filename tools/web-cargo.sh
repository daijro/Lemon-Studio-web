#!/usr/bin/env bash
# Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
# and not any other part of Lemon Studio.

set -euo pipefail
cd "$(dirname "$0")/.."
: "${EMSDK:?Set EMSDK to the Emscripten 4.0.23 SDK directory}"
export RUSTFLAGS="-C opt-level=3 -C target-feature=+atomics,+bulk-memory,+mutable-globals,+simd128"
# Rust stopped automatically exporting the WASM thread runtime in October 2025.
cargo +"${WEB_TOOLCHAIN:-nightly-2025-11-06}" rustc --locked -Zbuild-std=std,panic_abort --target wasm32-unknown-unknown "$@" --   -C link-arg=--shared-memory -C link-arg=--import-memory -C link-arg=--max-memory=2147483648   -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size   -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base
