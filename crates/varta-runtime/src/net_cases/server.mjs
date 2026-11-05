// The Node mirror of `net_loopback.rs`: the same routes and responses on
// 127.0.0.1, for producing the Node 24 expectations pinned in
// `net_tests.rs` and for the browser acceptance probes
// (`scripts/test-browser-streaming.mjs` imports `route`).
// Usage: node server.mjs PORT
import http from 'node:http';
import { gzipSync } from 'node:zlib';
import { pathToFileURL } from 'node:url';

const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const CHUNK_PAUSE = 20;
// The `User-Agent` marker `/agent` looks for (`net_loopback::AGENT_MARKER`).
const AGENT_MARKER = 'varta-test-sdk';

// One server-sent event: `size` data bytes, then its id (`net_loopback::event`).
export const event = (id, size) => `data: ${'x'.repeat(size)}\nid: ${id}\n\n`;
const parameter = (query, name) => Number(new URLSearchParams(query ?? '').get(name) ?? 0);

// Answer one request; `headers` are added to every response (CORS).
export async function route(req, res, headers = {}) {
  const parts = [];
  for await (const chunk of req) parts.push(chunk);
  const raw = Buffer.concat(parts);
  const body = raw.toString('utf8');
  const [path, query] = req.url.split('?');
  const fixed = (status, reason, fields, data) => {
    res.writeHead(status, reason, { ...headers, ...fields, 'content-length': Buffer.byteLength(data), connection: 'close' });
    res.end(data);
  };
  const chunked = async (type, chunks) => {
    res.writeHead(200, 'OK', { ...headers, 'content-type': type, connection: 'close' });
    for (const chunk of chunks) {
      await pause(CHUNK_PAUSE);
      res.write(chunk);
    }
    res.end();
  };
  // Each chunk after its own pause.
  const events = async (chunks) => {
    res.writeHead(200, 'OK', { ...headers, 'content-type': 'text/event-stream', connection: 'close' });
    for (const [wait, chunk] of chunks) {
      await pause(wait);
      res.write(chunk);
    }
    res.end();
  };
  switch (path) {
    case '/hello':
      return fixed(200, 'OK', { 'content-type': 'text/plain', 'x-test': 'a' }, 'hello world');
    case '/json':
      return fixed(201, 'Created', { 'content-type': 'application/json' }, '{"ok":true,"items":[1,2,3]}');
    case '/echo':
      return fixed(200, 'OK', { 'content-type': 'application/json' }, JSON.stringify({
        method: req.method,
        auth: req.headers.authorization ?? null,
        type: req.headers['content-type'] ?? null,
        query: query ?? null,
        body,
      }));
    case '/sse':
      return chunked('text/event-stream', ['id: 1\ndata: tick 1\n\n', 'id: 2\ndata: tick 2\n\n', 'id: 3\ndata: tick 3\n\n']);
    case '/utf8': {
      const bytes = Buffer.from('é€😀', 'utf8');
      return chunked('text/plain; charset=utf-8', [bytes.subarray(0, 1), bytes.subarray(1, 4), bytes.subarray(4)]);
    }
    case '/bytes':
      return fixed(200, 'OK', { 'content-type': 'application/octet-stream' }, Buffer.from([0x00, 0xff, 0x80, 0x41]));
    case '/mirror':
      return fixed(200, 'OK', req.headers['content-type'] ? { 'content-type': req.headers['content-type'] } : {}, raw);
    case '/redirect':
      return fixed(302, 'Found', { location: '/hello' }, '');
    case '/missing':
      return fixed(404, 'Not Found', { 'content-type': 'text/plain' }, 'nope');
    case '/slow':
      return;
    // V38 long-lived streams and host-owned request headers.
    case '/events': {
      const wait = parameter(query, 'pause'), size = parameter(query, 'size');
      return events(Array.from({ length: parameter(query, 'count') }, (_, i) => [wait, event(i + 1, size)]));
    }
    case '/stall':
      return events([[0, event(1, 1)], [parameter(query, 'gap'), event(2, 1)]]);
    case '/agent':
      return fixed(200, 'OK', { 'content-type': 'application/json' }, JSON.stringify({ guestAgent: (req.headers['user-agent'] ?? '').includes(AGENT_MARKER) }));
    case '/sent':
      return fixed(200, 'OK', { 'content-type': 'application/json' }, JSON.stringify({ agent: req.headers['user-agent'] ?? null, encoding: req.headers['accept-encoding'] ?? null }));
    case '/coded':
      return fixed(200, 'OK', { 'content-type': 'text/plain', 'content-encoding': 'gzip' }, 'coded');
    // Browser only: compressed when the request accepts gzip. The native
    // host asks for the identity coding, so `net_loopback.rs` needs no twin.
    case '/compressed': {
      const text = 'compressed text';
      if (!(req.headers['accept-encoding'] ?? '').includes('gzip')) return fixed(200, 'OK', { 'content-type': 'text/plain' }, text);
      return fixed(200, 'OK', { 'content-type': 'text/plain', 'content-encoding': 'gzip' }, gzipSync(text));
    }
    default:
      return fixed(500, 'Internal Server Error', {}, '');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const server = http.createServer((req, res) => { route(req, res); });
  server.listen(Number(process.argv[2] ?? 0), '127.0.0.1', () => console.log(`http://127.0.0.1:${server.address().port}`));
}
