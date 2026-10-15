#!/usr/bin/env python3
# Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
# and not any other part of Lemon Studio.

from pathlib import Path
import re
import sys
import zipfile

root, archive = map(Path, sys.argv[1:])
required = {"index.html", "bootstrap.mjs", "os_app.js", "os_app_bg.wasm", "utf8.mjs", "_headers", "LICENSE"}
assert all((root / name).is_file() for name in required), "Missing web runtime asset or license"
headers = (root / "_headers").read_text()
assert "Cross-Origin-Opener-Policy: same-origin" in headers
assert "Cross-Origin-Embedder-Policy: require-corp" in headers
for file in root.rglob("*"):
    if file.suffix in {".js", ".mjs", ".html"}:
        text = file.read_text()
        assert not re.search(r"(?:from\s+|import\s*\(|new URL\(|src=)\s*['\"](?:https?://|/)", text), f"Nonlocal executable dependency: {file}"
assert archive.is_file(), "Missing web ZIP"
with zipfile.ZipFile(archive) as package:
    assert package.testzip() is None
    for name in package.namelist():
        assert not name.startswith("/") and ".." not in Path(name).parts
    for file in root.rglob("*"):
        if file.is_file():
            assert package.read(file.relative_to(root).as_posix()) == file.read_bytes(), f"Package differs: {file}"
print("PASS: complete local web assets, isolation headers, licenses and matching ZIP")
