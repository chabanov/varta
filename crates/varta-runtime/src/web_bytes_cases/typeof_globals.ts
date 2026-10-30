// V38 `typeof` of engine global identifiers: classes and functions are
// "function", namespaces and objects "object", and the feature-detection
// guard of a binary upload narrows its body. Node 24 prints the same lines
// (`node typeof_globals.ts` after appending `await main();`).
type Body = string | Blob | Uint8Array;

// The guard shape of a typed SDK: the declared type unless a `Blob` carries
// its own.
function contentType(body: Body, declared: string): string {
  const own = typeof Blob !== 'undefined' && body instanceof Blob ? body.type.split(';')[0]!.trim().toLowerCase() : '';
  if (!own) return declared;
  return own;
}

function streams(): boolean {
  return typeof ReadableStream === 'function' && typeof TextDecoder !== 'undefined';
}

function shadowed(Blob: number): string {
  return typeof Blob;
}

function local(): string {
  const Map = 'local';
  return typeof Map;
}

export async function main(): Promise<void> {
  console.log(`${typeof Blob} ${typeof FormData} ${typeof ArrayBuffer} ${typeof Headers} ${typeof Map} ${typeof Promise} ${typeof TypeError}`);
  console.log(`${typeof fetch} ${typeof setTimeout} ${typeof encodeURIComponent} ${typeof parseInt} ${typeof btoa}`);
  console.log(`${typeof Math} ${typeof JSON} ${typeof console} ${typeof crypto} ${typeof globalThis} ${typeof process}`);
  console.log(contentType(new Blob(['x'], { type: 'Image/PNG; q=1' }), 'image/*'));
  console.log(contentType(new Uint8Array([1, 2]), 'image/*'));
  console.log(contentType('text', 'application/octet-stream'));
  console.log(`${streams()} ${typeof URL === 'undefined' ? 'no url' : new URL('https://a.test/x').pathname}`);
  console.log(`${local()} ${shadowed(1)} ${typeof (Uint8Array)}`);
}
