// V38 byte bodies (`web_bytes`): `Uint8Array` chunks from a reader,
// `arrayBuffer()`, `bytes()` and `blob()`, a stream wrapper built from an
// `UnderlyingSource`, and request bodies from bytes, a `Blob` and
// `FormData`. Node runs it with `node_runner.mjs --bytes` (Node's own byte
// chunks); the multipart boundary is the one line that differs (Node draws a
// random one, Varta's is deterministic), so the program prints it masked.
const BASE = '__BASE__';

function list(bytes: Uint8Array): string {
  const parts: string[] = [];
  for (const byte of bytes) {
    parts.push(String(byte));
  }
  return parts.join(',');
}

async function chunks(): Promise<void> {
  const response = await fetch(BASE + '/utf8');
  const body = response.body;
  if (body === null) {
    return;
  }
  const reader = body.getReader();
  const decoder = new TextDecoder('utf-8');
  let total = 0;
  let text = '';
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    total += value.length;
    text += decoder.decode(value, { stream: true });
  }
  text += decoder.decode();
  reader.releaseLock();
  console.log(`${total} ${text} ${body.locked}`);
}

async function whole(): Promise<void> {
  const raw = await fetch(BASE + '/bytes');
  const bytes = await raw.bytes();
  console.log(`bytes ${list(bytes)}`);
  const again = await fetch(BASE + '/bytes');
  const buffer = await again.arrayBuffer();
  console.log(`arrayBuffer ${buffer.byteLength} ${list(new Uint8Array(buffer))}`);
  const third = await fetch(BASE + '/bytes');
  const blob = await third.blob();
  const sliced = await blob.slice(1, 3).bytes();
  console.log(`blob ${blob.size} ${blob.type} ${list(sliced)}`);
  console.log(`used ${third.bodyUsed}`);
}

// The pilot's wrapper: a pull source over another stream's reader.
function wrapped(body: ReadableStream<Uint8Array>): ReadableStream<Uint8Array> {
  const reader = body.getReader();
  return new ReadableStream<Uint8Array>({
    async pull(controller) {
      let chunk: ReadableStreamReadResult<Uint8Array>;
      try {
        chunk = await reader.read();
      } catch (error) {
        controller.error(error);
        return;
      }
      if (chunk.done) controller.close();
      else controller.enqueue(chunk.value);
    },
    cancel(reason) {
      return reader.cancel(reason);
    },
  });
}

async function wrapper(): Promise<void> {
  const response = await fetch(BASE + '/sse');
  const body = response.body;
  if (body === null) {
    return;
  }
  const copy = new Response(wrapped(body), {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
  });
  const text = await copy.text();
  console.log(`wrapped ${copy.status} ${copy.headers.get('content-type')} ${text.length}`);
}

async function sent(): Promise<void> {
  const bytes = await fetch(BASE + '/mirror', {
    method: 'POST',
    body: new Uint8Array([0, 255, 128, 65]),
  });
  const echoed = await bytes.bytes();
  console.log(`sent bytes ${bytes.headers.get('content-type')} ${list(echoed)}`);
  const blob = await fetch(BASE + '/mirror', {
    method: 'PUT',
    body: new Blob(['héllo'], { type: 'Text/Plain' }),
  });
  const blobText = await blob.text();
  console.log(`sent blob ${blob.headers.get('content-type')} ${blobText}`);
  const form = new FormData();
  form.append('name', 'value');
  form.append('file', new Blob(['abc'], { type: 'text/plain' }), 'a.txt');
  const multipart = await fetch(BASE + '/mirror', { method: 'POST', body: form });
  const type = multipart.headers.get('content-type') ?? '';
  const boundary = type.slice(type.indexOf('boundary=') + 'boundary='.length);
  const text = await multipart.text();
  const masked = text.split(boundary).join('<b>');
  console.log(`sent form ${type.slice(0, type.indexOf(';'))} ${JSON.stringify(masked)}`);
}

export async function main(): Promise<void> {
  await chunks();
  await whole();
  await wrapper();
  await sent();
}
