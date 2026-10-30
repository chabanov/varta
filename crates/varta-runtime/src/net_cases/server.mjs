// The Node mirror of `net_loopback.rs`: the same routes and responses on
// 127.0.0.1, for producing the Node 24 expectations pinned in
// `net_tests.rs`. Usage: node server.mjs PORT
import http from 'node:http';

const port = Number(process.argv[2] ?? 0);
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const CHUNK_PAUSE = 20;

const server = http.createServer(async (req, res) => {
  const parts = [];
  for await (const chunk of req) parts.push(chunk);
  const raw = Buffer.concat(parts);
  const body = raw.toString('utf8');
  const [path, query] = req.url.split('?');
  const fixed = (status, reason, headers, data) => {
    res.writeHead(status, reason, { ...headers, 'content-length': Buffer.byteLength(data), connection: 'close' });
    res.end(data);
  };
  const chunked = async (type, chunks) => {
    res.writeHead(200, 'OK', { 'content-type': type, connection: 'close' });
    for (const chunk of chunks) {
      await pause(CHUNK_PAUSE);
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
    default:
      return fixed(500, 'Internal Server Error', {}, '');
  }
});
server.listen(port, '127.0.0.1', () => console.log(`http://127.0.0.1:${server.address().port}`));
