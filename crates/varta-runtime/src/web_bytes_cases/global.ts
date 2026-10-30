// V38 typed `globalThis` in the pilot's feature-detection idioms. Node 24
// prints the same lines (`node global.ts` after appending `await main();`).
type FetchLike = (input: string, init: RequestInit) => Promise<Response>;

function randomId(): string {
  const c = globalThis.crypto;
  if (c && typeof c.randomUUID === 'function') return c.randomUUID();
  return 'none';
}

function readEnv(): (name: string) => string | undefined {
  const env = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process?.env;
  return (name) => env?.[name];
}

export async function main(): Promise<void> {
  const id = randomId();
  console.log(`${id.length} ${id.charAt(14)} ${id.split('-').length}`);
  const bytes = new Uint8Array(4);
  globalThis.crypto.getRandomValues(bytes);
  console.log(`${bytes.length}`);
  const fetchImpl = globalThis.fetch as FetchLike | undefined;
  console.log(fetchImpl ? 'fetch' : 'no fetch');
  console.log(`${typeof globalThis.encodeURIComponent} ${globalThis.encodeURIComponent('a b')}`);
  const env = readEnv();
  console.log(`${env('VARTA_TEST_UNSET_VARIABLE')}`);
  console.log(`${globalThis.globalThis.crypto.randomUUID().length}`);
}
