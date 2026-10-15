#!/usr/bin/env python3
# Copyright (C) 2026 ItsAlphaNeon. Lemon Studio: https://github.com/ItsAlphaNeon/Lemon-Studio
# SPDX-License-Identifier: GPL-3.0-or-later
#
# This code, in its entirety, is NOT to be used as part of the ArtCraft suite: not this file, not any part of it,
# and not any other part of Lemon Studio.

import argparse
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

class Handler(SimpleHTTPRequestHandler):
    extensions_map = {**SimpleHTTPRequestHandler.extensions_map, ".wasm": "application/wasm", ".mjs": "text/javascript"}
    def end_headers(self):
        self.send_header("Cross-Origin-Opener-Policy", "same-origin")
        self.send_header("Cross-Origin-Embedder-Policy", "require-corp")
        super().end_headers()

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Serve Lemon Studio with browser audio isolation headers")
    parser.add_argument("--directory", default="dist/web")
    parser.add_argument("--port", type=int, default=8080)
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", args.port), partial(Handler, directory=args.directory))
    print(f"Lemon Studio: http://127.0.0.1:{args.port}/", flush=True)
    try: server.serve_forever()
    except KeyboardInterrupt: pass
    finally: server.server_close()
