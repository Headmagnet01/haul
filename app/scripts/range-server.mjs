// A tiny HTTP server for the real-app smoke run. No dependencies.
// Serves any /<name>.bin path (the same file): 64 MiB of deterministic bytes ((i * 31 + 7) % 251, as in the engine tests),
// and /fixtures/<name> from crates/engine/tests/fixtures/archives (the post-processing test archives),
// with Accept-Ranges, 206 + Content-Range, an ETag, and a per-response throttle.
// Usage: node scripts/range-server.mjs <port> [bytesPerSecondPerResponse=1000000]
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { basename, dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const FIXTURES = join(dirname(fileURLToPath(import.meta.url)), '../../crates/engine/tests/fixtures/archives');

const port = Number(process.argv[2] ?? 8765);
const bps = Number(process.argv[3] ?? 1_000_000);
const BIG = 64 * 1024 * 1024;
const big = Buffer.alloc(BIG);
for (let i = 0; i < BIG; i++) big[i] = (i * 31 + 7) % 251;
const bigEtag = '"haul-smoke-64mib"';
const sha = createHash('sha256').update(big).digest('hex');

const server = createServer((req, res) => {
  const path = new URL(req.url ?? '/', 'http://x').pathname;
  let data = big;
  let etag = bigEtag;
  if (path.startsWith('/fixtures/')) {
    const file = join(FIXTURES, basename(decodeURIComponent(path)));
    if (!existsSync(file)) {
      res.writeHead(404).end();
      return;
    }
    data = readFileSync(file);
    etag = `"fixture-${data.length}"`;
  } else if (!path.endsWith('.bin')) {
    res.writeHead(404).end();
    return;
  }
  const SIZE = data.length;
  const range = req.headers.range;
  let start = 0;
  let end = SIZE - 1;
  const head = { 'Accept-Ranges': 'bytes', ETag: etag, 'Content-Type': 'application/octet-stream' };
  if (range) {
    const m = /^bytes=(\d+)-(\d*)$/.exec(range);
    if (!m || Number(m[1]) >= SIZE) {
      res.writeHead(416, { ...head, 'Content-Range': `bytes */${SIZE}` }).end();
      return;
    }
    start = Number(m[1]);
    end = m[2] ? Math.min(Number(m[2]), SIZE - 1) : SIZE - 1;
    res.writeHead(206, { ...head, 'Content-Length': end - start + 1, 'Content-Range': `bytes ${start}-${end}/${SIZE}` });
  } else {
    res.writeHead(200, { ...head, 'Content-Length': SIZE });
  }
  console.log(`${req.method} ${req.url} range=${range ?? '-'}`);
  if (req.method === 'HEAD') {
    res.end();
    return;
  }
  const chunk = Math.max(1, Math.floor(bps / 20)); // 20 writes per second
  let pos = start;
  const timer = setInterval(() => {
    if (res.destroyed) {
      clearInterval(timer);
      return;
    }
    const next = Math.min(end + 1, pos + chunk);
    res.write(data.subarray(pos, next));
    pos = next;
    if (pos > end) {
      clearInterval(timer);
      res.end();
    }
  }, 50);
  res.on('close', () => clearInterval(timer));
});

server.listen(port, '127.0.0.1', () => console.log(`range-server on ${port}, ${bps} B/s per response, sha256 ${sha}`));
