#!/usr/bin/env python3
"""Dev server with HTTP Range request support and permissive CSP headers for v86 JIT + SharedArrayBuffer."""
import http.server, io, os, sys

class Handler(http.server.SimpleHTTPRequestHandler):
    extensions_map = http.server.SimpleHTTPRequestHandler.extensions_map.copy()
    extensions_map.update({
        '.wasm': 'application/wasm',
        '.iso': 'application/x-iso9660-image',
        '.bin': 'application/octet-stream',
        '.img': 'application/octet-stream',
        '.js': 'application/javascript',
        '.mjs': 'application/javascript',
        '.json': 'application/json',
        '.apk': 'application/vnd.android.package-archive',
    })

    def end_headers(self):
        self.send_header('Accept-Ranges', 'bytes')
        self.send_header('Cross-Origin-Opener-Policy', 'same-origin')
        self.send_header('Cross-Origin-Embedder-Policy', 'require-corp')
        self.send_header('Content-Security-Policy',
            "default-src 'self' blob: data:; "
            "script-src 'self' 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval' blob:; "
            "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; "
            "font-src 'self' data: https://fonts.gstatic.com; "
            "img-src 'self' blob: data:; "
            "connect-src 'self' blob: data: ws: wss:; "
            "worker-src 'self' blob:;")
        super().end_headers()

    def send_head(self):
        path = self.translate_path(self.path)
        if os.path.isdir(path):
            return super().send_head()

        ctype = self.guess_type(path)
        try:
            f = open(path, 'rb')
        except OSError:
            self.send_error(404, 'File not found')
            return None

        fs = os.fstat(f.fileno())
        total = fs.st_size
        range_header = self.headers.get('Range')

        if range_header and range_header.startswith('bytes='):
            range_val = range_header[6:].strip()
            try:
                if '-' in range_val:
                    start_str, end_str = range_val.split('-', 1)
                    if start_str and end_str:
                        start = int(start_str)
                        end = int(end_str)
                    elif start_str:
                        start = int(start_str)
                        end = total - 1
                    elif end_str:
                        suffix_len = int(end_str)
                        start = max(0, total - suffix_len)
                        end = total - 1
                    else:
                        raise ValueError('Invalid range')
                else:
                    raise ValueError('Invalid range')

                if start >= total or start > end or end >= total or start < 0:
                    f.close()
                    self.send_response(416, 'Range Not Satisfiable')
                    self.send_header('Content-Range', f'bytes */{total}')
                    self.end_headers()
                    return None

                length = end - start + 1
                self.send_response(206, 'Partial Content')
                self.send_header('Content-Type', ctype)
                self.send_header('Content-Range', f'bytes {start}-{end}/{total}')
                self.send_header('Content-Length', str(length))
                self.end_headers()
                f.seek(start)
                return io.BytesIO(f.read(length))
            except Exception:
                f.close()
                self.send_response(416, 'Range Not Satisfiable')
                self.send_header('Content-Range', f'bytes */{total}')
                self.end_headers()
                return None
        else:
            self.send_response(200)
            self.send_header('Content-Type', ctype)
            self.send_header('Content-Length', str(total))
            self.end_headers()
            return f

http.server.HTTPServer(('', int(sys.argv[1]) if len(sys.argv) > 1 else 8080), Handler).serve_forever()
