#!/usr/bin/env bash
# Copyright (C) 2026 ItsAlphaNeon
# SPDX-License-Identifier: GPL-3.0-or-later
# This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

set -euo pipefail
cd "$(dirname "$0")/.."
wasm="$1"
output="$2"
"${WASM_BINDGEN:-wasm-bindgen}" --target web --out-dir "$output" "$wasm"
cp web/utf8.mjs "$output/utf8.mjs"
node --input-type=module - "$output" "$(basename "$wasm" .wasm)" <<'JS'
import {readFileSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
const file=join(process.argv[2], process.argv[3]+'.js');
writeFileSync(file, `import { TextDecoder, TextEncoder } from './utf8.mjs';\n`+readFileSync(file,'utf8'));
JS
