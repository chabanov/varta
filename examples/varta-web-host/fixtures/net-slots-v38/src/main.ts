// v38 checks the pending-request limit before the broker: with every slot held
// by a request the server never answers, a denied request is fatal and its
// denial is never written. The test rewrites the grant to the server's port.
// The slot count is the contract's "Requests holding a runtime slot at once".
const SLOTS = 8;

export async function main(): Promise<void> {
  const base = varta.env.get("BASE_URL");
  const held: Promise<Response>[] = [];
  for (let i = 0; i < SLOTS; i += 1) held.push(fetch(base + "/hang"));
  console.log(String(held.length) + " requests in flight");
  try {
    await fetch("http://127.0.0.2:9/x");
  } catch (error) {
    console.log("the extra request threw");
  }
}
