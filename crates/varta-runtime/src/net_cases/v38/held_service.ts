// A Varta HTTP service for the abort tests (`net_tests.rs`, with
// `abort_close.ts` as its client): `/held` keeps its worker busy on a request
// to `HELD`, the harness's loopback route that never answers (and that lets
// the client's gate see the worker holding it); every other target answers
// at once. The harness replaces the placeholder.
const HELD = '__HELD__';

async function handle(request: VartaHttpRequest): Promise<VartaHttpResponse> {
  if (request.target === '/held') {
    const upstream = await fetch(HELD);
    return { status: 200, body: await upstream.text(), headers: [] };
  }
  return { status: 200, body: `served ${request.target}`, headers: [] };
}

export function main(): void {}

varta.http.serve({ hostname: '127.0.0.1', port: 0 }, handle);
