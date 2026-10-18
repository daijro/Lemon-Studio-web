#!/usr/bin/env bash
# Copyright (C) 2026 ItsAlphaNeon
# SPDX-License-Identifier: GPL-3.0-or-later
# This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
web_tools="${WEB_TOOLS_DIR:-$CARGO_TARGET_DIR/web-tools}"
export EMSDK="${EMSDK:-$web_tools/emsdk}"
export WASM_BINDGEN="${WASM_BINDGEN:-$web_tools/bin/wasm-bindgen}"
if [[ ! -x "$WASM_BINDGEN" ]] || [[ "$("$WASM_BINDGEN" --version)" != 'wasm-bindgen 0.2.129' ]]; then
    echo 'Install the pinned web tooling with tools/web-setup.sh first.' >&2; exit 1
fi
output="${WEB_BUILD_DIR:-$PWD/dist/web}"
mkdir -p "$output/licenses"
tools/web-cargo.sh -p os-app --lib --release "$@"
tools/web-bindgen.sh "$CARGO_TARGET_DIR/wasm32-unknown-unknown/release/os_app.wasm" "$output"
cp web/index.html web/bootstrap.mjs web/_headers LICENSE "$output/"
cp vendor/nih-plug/LICENSE "$output/licenses/NIH-plug.txt"
cp vendor/cpal/LICENSE "$output/licenses/CPAL.txt"
cp crates/os-ui/fonts/Cuprum/OFL.txt "$output/licenses/Cuprum.txt"
cp crates/os-ui/fonts/Barlow/OFL.txt "$output/licenses/Barlow.txt"
cp crates/os-ui/icons/TABLER-LICENSE.txt "$output/licenses/Tabler.txt"
cp crates/os-dsp/vendor/signalsmith/LICENSE-wrapper.md "$output/licenses/Signalsmith-wrapper.md"
cp crates/os-dsp/vendor/signalsmith/signalsmith-stretch/LICENSE.txt "$output/licenses/Signalsmith-stretch.txt"
cp crates/os-dsp/vendor/signalsmith/signalsmith-linear/LICENSE.txt "$output/licenses/Signalsmith-linear.txt"
archive="$(dirname "$output")/lemon-studio-web.zip"
python3 - "$output" "$archive" <<'PYZIP'
from pathlib import Path
from zipfile import ZipFile, ZIP_DEFLATED
import sys, gzip
root=Path(sys.argv[1])
for file in sorted(root.rglob("*")):
    if file.suffix in {".js", ".mjs", ".wasm", ".html", ".css"}:
        file.with_name(file.name + ".gz").write_bytes(gzip.compress(file.read_bytes(), compresslevel=9, mtime=0))
with ZipFile(sys.argv[2], 'w', ZIP_DEFLATED) as z:
    for file in sorted(root.rglob('*')):
        if file.is_file(): z.write(file,file.relative_to(root))
PYZIP
python3 tools/web-package-check.py "$output" "$archive"
printf 'Web app: %s\nWeb ZIP: %s\n' "$output" "$archive"
