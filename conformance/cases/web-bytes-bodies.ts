// V38 Web bytes without the network: Blob parts mixing text and bytes,
// slices, ArrayBuffer copies, Response bodies read as text, bytes and JSON,
// and a Request built with a byte body.
export async function main(): Promise<void> {
  const encoded = new TextEncoder().encode('\u20acuro');
  const blob = new Blob([encoded, ' and ', new Uint8Array([0x41, 0x42])], { type: 'TEXT/Plain' });
  console.log(`${encoded.length} ${blob.size} ${blob.type}`);
  console.log(await blob.text());
  const middle = blob.slice(3, 8);
  const middleBytes = await middle.bytes();
  console.log(`${middle.size} ${middleBytes.join(' ')} ${blob.slice(-2).size} ${blob.slice(6, 2).size}`);
  const buffer = await blob.arrayBuffer();
  const copy = buffer.slice(0, 3);
  const view = new Uint8Array(copy);
  view[0] = 0x24;
  console.log(`${buffer.byteLength} ${copy.byteLength} ${view.join(',')} ${new Uint8Array(buffer)[0]}`);
  const json = new Response(new Uint8Array([123, 34, 110, 34, 58, 49, 125]), {
    status: 201,
    headers: { 'Content-Type': 'application/json' },
  });
  console.log(`${json.status} ${json.ok} ${json.bodyUsed} ${json.headers.get('content-type')}`);
  const value = (await json.json()) as { n: number };
  console.log(`${value.n} ${json.bodyUsed}`);
  try {
    await json.text();
  } catch (error) {
    console.log(`reread ${error instanceof TypeError}`);
  }
  const fromBlob = new Response(new Blob(['xyz'], { type: 'text/x-sample' }));
  const bytes = await fromBlob.bytes();
  console.log(`${bytes instanceof Uint8Array} ${bytes.length} ${fromBlob.headers.get('content-type')}`);
  const fromBuffer = new Response(new ArrayBuffer(3));
  console.log(`${(await fromBuffer.bytes()).join(',')} ${fromBuffer.headers.get('content-type')}`);
  const upload = new Request('https://example.test/upload', { method: 'PUT', body: new Uint8Array([1, 2, 3]) });
  console.log(`${upload.method} ${upload.url}`);
}
