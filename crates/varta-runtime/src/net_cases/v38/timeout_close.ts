// A transfer that fails on its own (its head or its next chunk does not
// arrive within the inactivity window) closes its connection before the
// failure reaches the guest, not when the event loop next drives the
// network. The program then waits on a timer alone (no request in flight),
// so only the failure can close the connection before `/hello` is sent. The
// harness (`net_tests.rs`) replaces the placeholders and serves `PATH`
// (`/held`, which never answers, or `/trickle`, which sends one event and
// keeps silent) and `/hello` from its loopback server.
const BASE = '__BASE__';
const PATH = '__PATH__';
const PAUSE_MS = __PAUSE__;

export async function main(): Promise<void> {
  try {
    const response = await fetch(BASE + PATH);
    const body = response.body;
    if (body !== null) {
      const reader = body.getReader();
      for (;;) {
        const { done } = await reader.read();
        if (done) {
          break;
        }
      }
    }
    console.log('held: answered');
  } catch (error) {
    console.log(`held: ${error instanceof TypeError ? error.message : 'other'}`);
  }
  await new Promise<void>((resolve) => {
    setTimeout(() => resolve(), PAUSE_MS);
  });
  const next = await fetch(BASE + '/hello');
  console.log(`hello: ${next.status}`);
}
