#!/usr/bin/env bash
# Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
# and not any other part of Lemon Studio.

set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
output="${WEB_BUILD_DIR:-$PWD/dist}"
mkdir -p "$output"
tools/web-cargo.sh -p os-app --lib "$@"
tools/web-bindgen.sh "$CARGO_TARGET_DIR/wasm32-unknown-unknown/debug/os_app.wasm" "$output"
cp web/index.html web/bootstrap.mjs "$output/"
printf 'Web app: %s\n' "$output"
