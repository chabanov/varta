// A v38 burst: large server-sent events sent back to back, so the host
// receives far more than one read may deliver at once. Run as an
// application instance with the production budgets: each read is one event
// turn, and the per-read delivery cap (`net::MAX_READ_DELIVERY_BYTES`) keeps
// its guest work within the turn fuel. The lines are host-independent, so
// the native and browser hosts print the same output. The harness replaces
// the placeholders: `net_tests.rs` and `scripts/test-browser-streaming.mjs`.
const BASE = '__BASE__';
const EVENTS = '/events?count=__COUNT__&size=__SIZE__&pause=0';
const CAP = __CAP__;
const TAIL_UNITS = 16;

async function bytes(): Promise<void> {
  const response = await fetch(BASE + EVENTS);
  const body = response.body;
  if (body === null) {
    console.log('bytes: no body');
    return;
  }
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let total = 0;
  let largest = 0;
  let tail = '';
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    total += value.length;
    largest = Math.max(largest, value.length);
    const last = value.slice(Math.max(0, value.length - TAIL_UNITS));
    tail = (tail + decoder.decode(last)).slice(-TAIL_UNITS);
  }
  const lines = tail.trim().split('\n');
  console.log(`bytes: ${response.status} ${total} bytes, last ${lines[lines.length - 1]}, reads within cap ${largest <= CAP}`);
}

async function text(): Promise<void> {
  const response = await fetch(BASE + EVENTS);
  const body = await response.text();
  // Only the tail: the application's own work stays small, as in `bytes`.
  const lines = body.slice(-TAIL_UNITS).trim().split('\n');
  console.log(`text: ${response.status} ${body.length} units, last ${lines[lines.length - 1]}`);
}

export async function main(): Promise<void> {
  await bytes();
  await text();
}
