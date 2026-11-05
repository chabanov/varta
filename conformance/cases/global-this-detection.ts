// V38 typed `globalThis` in feature-detection idioms of typed SDKs: an
// optional global fetch, `crypto.randomUUID`, an optional `process.env`
// read and `typeof` of engine globals.
type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

function pickFetch(custom: FetchLike | undefined): string {
  const chosen = custom ?? (globalThis.fetch as FetchLike | undefined);
  return chosen ? (custom ? 'custom' : 'global') : 'none';
}

function uuid(): string {
  const source = globalThis.crypto;
  if (source && typeof source.randomUUID === 'function') return source.randomUUID();
  return '';
}

function envReader(): (name: string) => string | undefined {
  const env = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env;
  return (name) => env?.[name];
}

export async function main(): Promise<void> {
  const custom: FetchLike = async () => new Response('ok');
  console.log(`${pickFetch(undefined)} ${pickFetch(custom)}`);
  const id = uuid();
  console.log(`${id.length} ${id.charAt(14)} ${id.split('-').length} ${id === uuid()}`);
  const random = new Uint8Array(8);
  const filled = globalThis.crypto.getRandomValues(random);
  console.log(`${filled.length} ${random.length}`);
  console.log(`${typeof fetch} ${typeof Blob} ${typeof TextDecoder} ${typeof ReadableStream} ${typeof AbortController}`);
  console.log(`${typeof JSON} ${typeof Math} ${typeof globalThis} ${typeof globalThis.encodeURIComponent}`);
  console.log(`${globalThis.encodeURIComponent('a b&c')} ${globalThis.encodeURI('/a b?c=\u20ac')}`);
  const env = envReader();
  console.log(`${env('VARTA_CONFORMANCE_UNSET_NAME')}`);
}
