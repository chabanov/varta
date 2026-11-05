// Multipart byte parts through the real `appendPart(form, key, value:
// unknown)` of uarp-sdk 0.13.2 (core/transport.ts encodeBody, appendPart and
// toBodyInit, copied unchanged): a `Uint8Array`, an `ArrayBuffer` and
// `{ data, filename, contentType }` file parts reach `unknown` inside the
// multipart record, then `'data' in`, `instanceof ArrayBuffer`,
// `ArrayBuffer.isView` and `as BlobPart` read them there. The request
// shapes are the SDK's: `file: BinaryInput` (generated models) and
// `FileInput` parts. The boundary is replaced by `<b>` (Node's is random,
// Varta's deterministic).
type BinaryInput = Blob | ArrayBuffer | ArrayBufferView | Uint8Array | string;
type FileInput = BinaryInput | { data: BinaryInput; filename?: string; contentType?: string };

interface RequestSpec {
  multipart?: object;
}

function encodeBody(spec: RequestSpec, headers: Headers): BodyInit | undefined {
  if (spec.multipart) {
    const form = new FormData();
    for (const [key, value] of Object.entries(spec.multipart as Record<string, unknown>)) {
      if (value === undefined || value === null) continue;
      appendPart(form, key, value);
    }
    // Let fetch set the multipart boundary.
    headers.delete('Content-Type');
    return form;
  }
  return undefined;
}

function appendPart(form: FormData, key: string, value: unknown): void {
  if (value instanceof Blob) {
    form.append(key, value);
    return;
  }
  if (typeof value === 'object' && value !== null && 'data' in (value as Record<string, unknown>)) {
    const part = value as { data: BinaryInput; filename?: string; contentType?: string };
    const blob = part.data instanceof Blob ? part.data : new Blob([toBodyInit(part.data) as BlobPart], {
      type: part.contentType ?? 'application/octet-stream',
    });
    form.append(key, blob, part.filename ?? 'file');
    return;
  }
  if (value instanceof ArrayBuffer || ArrayBuffer.isView(value)) {
    form.append(key, new Blob([value as BlobPart]), 'file');
    return;
  }
  form.append(key, typeof value === 'object' ? JSON.stringify(value) : String(value));
}

function toBodyInit(value: BinaryInput): BodyInit {
  if (typeof value === 'string') return value;
  if (value instanceof Blob) return value;
  if (value instanceof ArrayBuffer) return value;
  return value as ArrayBufferView as BodyInit;
}

async function wire(spec: RequestSpec): Promise<string> {
  const headers = new Headers({ 'Content-Type': 'application/json' });
  const body = encodeBody(spec, headers);
  const response = new Response(body);
  const type = response.headers.get('content-type') ?? '';
  const marker = 'boundary=';
  const boundary = type.slice(type.indexOf(marker) + marker.length);
  const bytes = new Uint8Array(await response.arrayBuffer());
  let text = '';
  for (const byte of bytes) text += String.fromCharCode(byte);
  // The byte count without the boundaries, whose length differs by engine.
  const shown = text.split(boundary).join('<b>');
  return `${String(headers.has('Content-Type'))} ${type.slice(0, type.indexOf(';'))} ${String(shown.length)} ${JSON.stringify(shown)}`;
}

interface UploadWorkspaceFileRequest {
  file: BinaryInput;
}

interface Upload {
  file: FileInput;
  note?: string;
  count?: number;
  meta?: { kind: string };
}

export async function main(): Promise<void> {
  const raw = new ArrayBuffer(3);
  const view = new Uint8Array(raw);
  view[0] = 0;
  view[1] = 128;
  view[2] = 255;

  const bytes: UploadWorkspaceFileRequest = { file: new Uint8Array([104, 105, 0, 200]) };
  console.log(await wire({ multipart: bytes }));
  const buffer: UploadWorkspaceFileRequest = { file: raw };
  console.log(await wire({ multipart: buffer }));
  const text: UploadWorkspaceFileRequest = { file: 'plain é' };
  console.log(await wire({ multipart: text }));

  const part: Upload = {
    file: { data: new Uint8Array([1, 2, 255]), filename: 'a.bin', contentType: 'application/x-test' },
    note: 'a\r\nb',
    count: 2,
    meta: { kind: 'x' },
  };
  console.log(await wire({ multipart: part }));
  const defaults: Upload = { file: { data: new Uint8Array([0]) } };
  console.log(await wire({ multipart: defaults }));
  const parts: Upload = { file: { data: raw, filename: 'r"aw\n.bin' } };
  console.log(await wire({ multipart: parts }));
  const blobPart: Upload = { file: { data: new Blob(['b'], { type: 'image/png' }), filename: 'b.png', contentType: 'text/plain' } };
  console.log(await wire({ multipart: blobPart }));
  const stringPart: Upload = { file: { data: 'é text' } };
  console.log(await wire({ multipart: stringPart }));
  const file: Upload = { file: new File(['ab', new Uint8Array([99])], 'notes.txt', { type: 'text/markdown' }) };
  console.log(await wire({ multipart: file }));
  const empty: UploadWorkspaceFileRequest = { file: new Uint8Array(0) };
  console.log(await wire({ multipart: empty }));
}
