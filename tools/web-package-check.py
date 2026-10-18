#!/usr/bin/env python3
# Copyright (C) 2026 ItsAlphaNeon
# SPDX-License-Identifier: GPL-3.0-or-later
# This file is part of Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio

from pathlib import Path
import re
import gzip
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
for file in root.rglob("*"):
    if file.suffix in {".js", ".mjs", ".wasm", ".html", ".css"}:
        compressed = file.with_name(file.name + ".gz")
        assert compressed.is_file(), f"Missing gzip asset: {file}"
        assert gzip.decompress(compressed.read_bytes()) == file.read_bytes(), f"Invalid gzip asset: {file}"
assert b"simd128" in (root / "os_app_bg.wasm").read_bytes(), "WASM SIMD was not enabled"
assert archive.is_file(), "Missing web ZIP"
with zipfile.ZipFile(archive) as package:
    assert package.testzip() is None
    for name in package.namelist():
        assert not name.startswith("/") and ".." not in Path(name).parts
    for file in root.rglob("*"):
        if file.is_file():
            assert package.read(file.relative_to(root).as_posix()) == file.read_bytes(), f"Package differs: {file}"
print("PASS: complete local web assets, isolation headers, licenses and matching ZIP")

# Exercise the shipped server, including clients that explicitly decline gzip.
import runpy
from functools import partial
from http.server import ThreadingHTTPServer
from threading import Thread
from urllib.request import Request, urlopen
handler = runpy.run_path(str(Path(__file__).with_name("web-serve.py")))["Handler"]
server = ThreadingHTTPServer(("127.0.0.1", 0), partial(handler, directory=str(root)))
Thread(target=server.serve_forever, daemon=True).start()
try:
    for encoding in ["gzip", "identity", "gzip;q=0"]:
        with urlopen(Request(f"http://127.0.0.1:{server.server_port}/os_app_bg.wasm", headers={"Accept-Encoding": encoding})) as response:
            body = response.read()
            assert response.headers["Content-Type"] == "application/wasm"
            assert response.headers["Cross-Origin-Opener-Policy"] == "same-origin"
            if encoding == "gzip":
                assert response.headers["Content-Encoding"] == "gzip", "Server did not use the gzip asset"
                body = gzip.decompress(body)
            else:
                assert response.headers["Content-Encoding"] is None
            assert body == (root / "os_app_bg.wasm").read_bytes()
finally:
    server.shutdown()
    server.server_close()
print("PASS: gzip content negotiation, MIME type and decoded bytes")
