// V38 long-lived transfers, run as an application instance whose per-turn
// deadline (the inactivity window) is far shorter than the stream:
// - `events` lasts several windows with a chunk inside every window and
//   carries more than one v37 response body, read chunk by chunk;
// - `stall` keeps silent for longer than one window, so its read fails;
// - `agent` sends an SDK `User-Agent`, which every host owns and ignores.
// The lines are host-independent (no chunk counts or sizes), so the native
// and browser hosts print the same output. The harness replaces the
// placeholders: `net_tests.rs` and `scripts/test-browser-streaming.mjs`.
// A v38-only program lives in this subdirectory, outside the
// `net_cases/*.ts` set whose v1-v37 compilations are frozen.
const BASE = '__BASE__';
const EVENTS = '/events?count=__COUNT__&size=__SIZE__&pause=__PAUSE__';
const STALL = '/stall?gap=__GAP__';
const TAIL_UNITS = 16;

async function stream(label: string, path: string): Promise<void> {
  const headers = new Headers();
  headers.set('Accept', 'text/event-stream');
  const response = await fetch(BASE + path, { headers });
  const body = response.body;
  if (body === null) {
    console.log(`${label}: no body`);
    return;
  }
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let bytes = 0;
  let tail = '';
  try {
    for (;;) {
      const { done, value } = await reader.read();
      if (done) {
        break;
      }
      bytes += value.length;
      const last = value.slice(Math.max(0, value.length - TAIL_UNITS));
      tail = (tail + decoder.decode(last)).slice(-TAIL_UNITS);
    }
    const lines = tail.trim().split('\n');
    console.log(`${label}: ${response.status} ${bytes} bytes, last ${lines[lines.length - 1]}`);
  } catch (error) {
    // The route ends cleanly after its silence, so only the inactivity
    // window can fail the read.
    const message = error instanceof Error ? error.message : '';
    const kind = error instanceof TypeError ? 'TypeError' : 'other';
    console.log(`${label}: ${kind} ${message} after ${bytes > 0 ? 'data' : 'nothing'}`);
  }
}

async function agent(): Promise<void> {
  const headers = new Headers();
  headers.set('User-Agent', 'varta-test-sdk/1.0 (uarp-sdk)');
  headers.set('Accept', 'application/json');
  const response = await fetch(BASE + '/agent', { headers });
  console.log(`agent: ${response.status} ${await response.text()}`);
}

// Compressed only when the request accepts gzip: a browser decodes it, the
// native host asks for the identity coding; the guest sees the same text and
// no coding either way.
async function compressed(): Promise<void> {
  const response = await fetch(BASE + '/compressed');
  const coding = response.headers.get('content-encoding');
  console.log(`compressed: ${response.status} ${await response.text()} ${coding === null ? 'no coding' : coding}`);
}

export async function main(): Promise<void> {
  await agent();
  await compressed();
  await stream('events', EVENTS);
  await stream('stall', STALL);
}
