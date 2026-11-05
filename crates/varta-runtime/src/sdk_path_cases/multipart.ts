// Multipart bodies as uarp-sdk 0.13.2 builds them (core/transport.ts
// encodeBody/appendPart/toBodyInit), with Blob, ArrayBuffer, Uint8Array,
// FileInput and File parts. `appendPart` receives `unknown`; the parts that
// reach it as `unknown` here are the ones Varta's v38 subset admits there
// (Blob, File and primitive or JSON fields). Byte parts take the Blob each
// SDK branch builds, so the wire bytes are the SDK's. The boundary is
// replaced by `<b>` (Node's is random, Varta's deterministic).
type BinaryInput = Blob | ArrayBuffer | ArrayBufferView | Uint8Array | string;
type FileInput = BinaryInput | { data: BinaryInput; filename?: string; contentType?: string };

function toBodyInit(value: BinaryInput): BodyInit {
  if (typeof value === 'string') return value;
  if (value instanceof Blob) return value;
  if (value instanceof ArrayBuffer) return value;
  return value as ArrayBufferView as BodyInit;
}

function appendPart(form: FormData, key: string, value: unknown): void {
  if (value instanceof Blob) {
    form.append(key, value);
    return;
  }
  form.append(key, typeof value === 'object' ? JSON.stringify(value) : String(value));
}

// The FileInput branch of appendPart, for a typed part.
function appendFile(form: FormData, key: string, part: { data: BinaryInput; filename?: string; contentType?: string }): void {
  const blob = part.data instanceof Blob ? part.data : new Blob([toBodyInit(part.data) as BlobPart], {
    type: part.contentType ?? 'application/octet-stream',
  });
  form.append(key, blob, part.filename ?? 'file');
}

// The ArrayBuffer/ArrayBufferView branch of appendPart, for a typed part.
function appendBytes(form: FormData, key: string, value: ArrayBuffer | Uint8Array): void {
  form.append(key, new Blob([value as BlobPart]), 'file');
}

function encode(multipart: Record<string, unknown>): FormData {
  const form = new FormData();
  for (const [key, value] of Object.entries(multipart)) {
    if (value === undefined || value === null) continue;
    appendPart(form, key, value);
  }
  return form;
}

async function wire(form: FormData): Promise<string> {
  const response = new Response(form);
  const type = response.headers.get('content-type') ?? '';
  const marker = 'boundary=';
  const boundary = type.slice(type.indexOf(marker) + marker.length);
  const bytes = new Uint8Array(await response.arrayBuffer());
  let text = '';
  for (const byte of bytes) text += String.fromCharCode(byte);
  // The byte count without the boundaries, whose length differs by engine.
  const body = text.split(boundary).join('<b>');
  return `${type.slice(0, type.indexOf(';'))} ${String(body.length)} ${JSON.stringify(body)}`;
}

interface Upload {
  file: Blob;
  note: string;
  count: number;
  tags: string[];
  skipped?: string;
}

export async function main(): Promise<void> {
  const upload: Upload = { file: new Blob(['héllo\n'], { type: 'Text/Plain' }), note: 'a\r\nb', count: 2, tags: ['x', 'y'] };
  console.log(await wire(encode({ ...upload })));

  const raw = new ArrayBuffer(3);
  const view = new Uint8Array(raw);
  view[0] = 0;
  view[1] = 128;
  view[2] = 255;
  const buffer = new FormData();
  appendBytes(buffer, 'buffer', raw);
  appendBytes(buffer, 'bytes', new Uint8Array([104, 105, 0, 200]));
  console.log(await wire(buffer));

  const files = new FormData();
  appendFile(files, 'data', { data: new Uint8Array([1, 2, 255]), filename: 'a.bin', contentType: 'application/x-test' });
  appendFile(files, 'text', { data: 'é text' });
  appendFile(files, 'buffer', { data: raw, filename: 'r"aw\n.bin' });
  appendFile(files, 'blob', { data: new Blob(['b'], { type: 'image/png' }), filename: 'ignored-type.png', contentType: 'text/plain' });
  console.log(await wire(files));

  const named = new FormData();
  const file = new File(['ab', new Uint8Array([99])], 'notes.txt', { type: 'text/markdown' });
  appendPart(named, 'file', file);
  named.append('renamed', file, 'other.md');
  named.append('untyped', new File([raw], 'raw.bin'));
  console.log(await wire(named));
}
