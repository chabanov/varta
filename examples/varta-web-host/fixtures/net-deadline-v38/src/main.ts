// A request the server never answers keeps the loop alive past a timer due
// beyond the deadline; v38 then fails with TimerDeadline, as without a request.
// The test rewrites the grant to the server's port.
const BEYOND_DEADLINE_MS = 3000;

export async function main(): Promise<void> {
  const base = varta.env.get("BASE_URL");
  const pending = fetch(base + "/hang");
  setTimeout(() => {
    console.log("timer fired");
  }, BEYOND_DEADLINE_MS);
  console.log("waiting");
  await pending;
  console.log("answered");
}
