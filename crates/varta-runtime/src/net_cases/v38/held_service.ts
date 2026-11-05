// A Varta HTTP service for the abort tests (`net_tests.rs`, with
// `abort_close.ts` as its client): `/held` keeps its worker busy on a timer
// that is due long after the client aborts, but before the request deadline;
// every other target answers at once. The harness replaces the placeholder.
const HELD_MS = __HELD__;

async function handle(request: VartaHttpRequest): Promise<VartaHttpResponse> {
  if (request.target === '/held') {
    await new Promise<void>((resolve) => {
      setTimeout(() => resolve(), HELD_MS);
    });
  }
  return { status: 200, body: `served ${request.target}`, headers: [] };
}

export function main(): void {}

varta.http.serve({ hostname: '127.0.0.1', port: 0 }, handle);
