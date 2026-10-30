// Run one `net_cases/*.ts` program under Node 24 against `server.mjs`, the
// way `net_tests.rs` runs it under Varta. Response bodies are piped through
// TextDecoderStream, because Varta's body chunks are decoded text where
// Node's are bytes; everything else is Node's own fetch.
// Usage: node node_runner.mjs BASE_URL CASE.ts [--bytes] (Node strips the
// types). With --bytes (the v38 byte-body cases) bodies stay Node's bytes.
import { readFileSync, writeFileSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [base, file, mode] = process.argv.slice(2);
const nodeFetch = globalThis.fetch;
if (mode !== '--bytes') globalThis.fetch = async (input, init) => {
  const response = await nodeFetch(input, init);
  const bytes = Object.getOwnPropertyDescriptor(Response.prototype, 'body').get;
  let text;
  // Decoded on first access, so text() and json() keep reading the bytes.
  Object.defineProperty(response, 'body', {
    get() {
      const body = bytes.call(response);
      text ??= body === null ? null : body.pipeThrough(new TextDecoderStream());
      return text;
    },
  });
  return response;
};
const source = readFileSync(file, 'utf8').replaceAll('__BASE__', base);
const directory = mkdtempSync(join(tmpdir(), 'varta-net-'));
const entry = join(directory, 'case.ts');
writeFileSync(entry, source);
const module = await import(pathToFileURL(entry).href);
await module.main();
