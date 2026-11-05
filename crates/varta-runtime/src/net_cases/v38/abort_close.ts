// Pilot finding F2: aborting a request while its head is pending closes its
// connection at once, not when the event loop next drives the network. The
// abort follows the answer of `GATE`, a loopback route that answers once the
// request for `/held` holds a connection there, so the abort meets a request
// in progress. The program then waits on a timer alone (no request in
// flight), so only the abort itself can close the connection before `/hello`
// is sent. The harness (`net_tests.rs`) replaces the placeholders and serves
// `/held` (never answered) and `/hello`, from its loopback server or from a
// Varta HTTP service whose `/held` holds a request to that loopback server.
const BASE = '__BASE__';
const GATE = '__GATE__';
const PAUSE_MS = __PAUSE__;

export async function main(): Promise<void> {
  const controller = new AbortController();
  const held = fetch(BASE + '/held', { signal: controller.signal });
  const gate = await fetch(GATE);
  console.log(`gate: ${gate.status} ${await gate.text()}`);
  controller.abort();
  try {
    await held;
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
