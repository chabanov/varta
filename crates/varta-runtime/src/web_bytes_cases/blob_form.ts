// V38 `Blob`, `ArrayBuffer` and `FormData` without the network. The same
// program runs under Node 24 (`node blob_form.ts` after appending
// `await main();`); only the multipart boundary differs (Node draws a random
// one, Varta's is deterministic), so the program prints it masked and then
// Varta's own boundary separately.
async function blobs(): Promise<void> {
  const blob = new Blob(['héllo ', new Uint8Array([119, 111]), 'rld'], { type: 'Text/Plain' });
  console.log(`${blob.size} ${blob.type}`);
  const text = await blob.text();
  console.log(text);
  const part = blob.slice(1, 4);
  const bytes = await part.bytes();
  console.log(`${part.size} ${bytes.join(',')} ${blob.slice(-3).size} ${blob.slice(5, 2).size}`);
  const buffer = await blob.arrayBuffer();
  const view = new Uint8Array(buffer);
  view[0] = 72;
  console.log(`${buffer.byteLength} ${new Uint8Array(buffer)[0]} ${buffer.slice(1, 3).byteLength}`);
  const nested = new Blob([blob, buffer, '\ud800'], { type: 'bad\u0001type' });
  console.log(`${nested.size} [${nested.type}]`);
}

async function forms(): Promise<void> {
  const form = new FormData();
  form.append('name', 'value\nline');
  form.append('file', new Blob(['abc'], { type: 'text/plain' }), 'a"b.txt');
  form.append('raw', new Blob(['xyz']));
  form.append('name', 'second');
  const names: string[] = [];
  for (const [name] of form.entries()) {
    names.push(name);
  }
  console.log(`${form.has('file')} ${form.getAll('name').length} ${names.join(',')}`);
  const got = form.get('file');
  console.log(got instanceof Blob ? `blob ${got.size} ${got.type}` : `${got}`);
  form.set('name', 'only');
  form.delete('raw');
  const only = form.get('name');
  const label = typeof only === 'string' ? only : 'blob';
  console.log(`${form.getAll('name').length} ${label} ${form.has('raw')} ${form.get('missing') === null}`);
  const response = new Response(form);
  const type = response.headers.get('content-type') ?? '';
  const text = await response.text();
  const boundary = type.slice(type.indexOf('boundary=') + 'boundary='.length);
  const masked = text.split(boundary).join('<b>');
  console.log(`${type.slice(0, type.indexOf(';'))} ${JSON.stringify(masked)}`);
}

async function bodies(): Promise<void> {
  const fromText = new Response('abc');
  const blob = await fromText.blob();
  console.log(`${blob.size} ${blob.type}`);
  const fromBytes = new Response(new Uint8Array([0xe2, 0x82, 0xac]));
  const euro = await fromBytes.text();
  console.log(`${euro} ${fromBytes.headers.get('content-type')}`);
  const fromBlob = new Response(new Blob(['{"a":1}'], { type: 'application/json' }));
  const json = await fromBlob.json();
  console.log(`${fromBlob.headers.get('content-type')} ${JSON.stringify(json)}`);
  const fromBuffer = new Response(new ArrayBuffer(2));
  const zeros = await fromBuffer.bytes();
  console.log(`${zeros.join(',')} ${fromBuffer.bodyUsed}`);
}

export async function main(): Promise<void> {
  await blobs();
  await forms();
  await bodies();
}
