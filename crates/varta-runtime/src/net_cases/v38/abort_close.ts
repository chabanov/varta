// Pilot finding F2: aborting a request while its head is pending closes its
// connection at once, not when the event loop next drives the network. The
// abort comes from a timer; the program then waits on a timer alone (no
// request in flight), so only the abort itself can close the connection
// before `/hello` is sent. The harness (`net_tests.rs`) replaces the
// placeholders and serves `/held` (never answered) and `/hello`, from its
// loopback server or from a Varta HTTP service.
const BASE = '__BASE__';
const ABORT_MS = __ABORT__;
const PAUSE_MS = __PAUSE__;

export async function main(): Promise<void> {
  const controller = new AbortController();
  setTimeout(() => controller.abort(), ABORT_MS);
  try {
    await fetch(BASE + '/held', { signal: controller.signal });
    console.log('held: answered');
  } catch (error) {
    console.log(`held: ${error instanceof DOMException ? error.name : 'other'}`);
  }
  await new Promise<void>((resolve) => {
    setTimeout(() => resolve(), PAUSE_MS);
  });
  const next = await fetch(BASE + '/hello');
  console.log(`hello: ${next.status}`);
}
