#!/usr/bin/env bash
# Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
# and not any other part of Lemon Studio.

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
