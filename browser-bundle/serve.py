#!/usr/bin/env python3
"""Dev server for the QEMU-WASM harness.

Sets Cross-Origin-Opener-Policy / Cross-Origin-Embedder-Policy so that
SharedArrayBuffer is available (required when the Emscripten build uses
pthreads, which qemu-system-aarch64 builds typically do), plus the
correct MIME type for .wasm.

Usage:  python3 serve.py [port]   (default 8124)
"""
import http.server
import sys

PORT = int(sys.argv[1]) if len(sys.argv) > 1 else 8124


class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = {
        **http.server.SimpleHTTPRequestHandler.extensions_map,
        '.wasm': 'application/wasm',
        '.js': 'text/javascript',
    }

    def end_headers(self):
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        super().end_headers()


if __name__ == '__main__':
    with http.server.ThreadingHTTPServer(('127.0.0.1', PORT), Handler) as httpd:
        print(f'serving on http://127.0.0.1:{PORT}/ (COOP/COEP enabled)')
        httpd.serve_forever()
