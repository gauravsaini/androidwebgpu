/**
 * serve.mjs - COOP/COEP static server for the browser bundle.
 * SharedArrayBuffer (guest RAM zero-copy) requires cross-origin isolation:
 *   Cross-Origin-Opener-Policy: same-origin
 *   Cross-Origin-Embedder-Policy: require-corp
 * The bare `python3 -m http.server` sends neither header, so real-worker
 * boot fails GUEST_RAM_NOT_SHARED under it. Use `make serve` / `npm run serve`.
 * Importable: `import { startServer } from './serve.mjs'` (SUT suites).
 */
import { createServer } from 'node:http';
import { readFileSync, existsSync, statSync } from 'node:fs';
import { extname, join, normalize } from 'node:path';

const MIME = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.wasm': 'application/wasm',
  '.json': 'application/json; charset=utf-8',
  '.img': 'application/octet-stream',
  '.bin': 'application/octet-stream',
};

export function createHandler(root) {
  return (req, res) => {
    const urlPath = decodeURIComponent(req.url.split('?')[0]);
    let rel = normalize(urlPath).replace(/^(\.\.[\/\\])+/, '').replace(/^\/+/, '');
    if (rel === '' || rel === '/') rel = 'index.html';
    const file = join(root, rel);
    res.setHeader('Cross-Origin-Opener-Policy', 'same-origin');
    res.setHeader('Cross-Origin-Embedder-Policy', 'require-corp');
    res.setHeader('Cross-Origin-Resource-Policy', 'same-origin');
    if (rel === 'favicon.ico') {
      res.writeHead(204);
      res.end();
      return;
    }
    if (!existsSync(file) || !statSync(file).isFile()) {
      res.writeHead(404, { 'Content-Type': 'text/plain' });
      res.end('NOT_FOUND');
      return;
    }
    const body = readFileSync(file);
    res.writeHead(200, {
      'Content-Type': MIME[extname(file)] || 'application/octet-stream',
      'Content-Length': body.byteLength,
    });
    if (req.method === 'HEAD') res.end();
    else res.end(body);
  };
}

export function startServer({ root = new URL('..', import.meta.url).pathname, port = 0 } = {}) {
  return new Promise((resolve) => {
    const server = createServer(createHandler(root));
    server.listen(port, '127.0.0.1', () => {
      resolve({ server, port: server.address().port });
    });
  });
}

const isMain = process.argv[1] && import.meta.url.endsWith(process.argv[1].split('/').pop());
if (isMain || process.argv.includes('--serve')) {
  const port = Number(process.env.PORT || 8089);
  startServer({ port }).then(({ port: p }) => {
    console.log(`SERVE_OK http://127.0.0.1:${p}/ (COOP/COEP enabled)`);
  });
}
