// v38 fetch over net.request against the test server. The test rewrites the
// grant to the server's port; BASE_URL names it.
// `Error.cause` is outside the checked subset, so a failure shows its name and message.
function describe(error: unknown): string {
  return error instanceof Error ? error.name + ": " + error.message : "non-error";
}

async function attempt(label: string, run: () => Promise<string>): Promise<void> {
  try {
    const result = await run();
    console.log(label + " -> " + result);
  } catch (error) {
    console.log(label + " threw " + describe(error));
  }
}

export async function main(): Promise<void> {
  const base = varta.env.get("BASE_URL");
  await attempt("json", async () => {
    const r = await fetch(base + "/json");
    const body = await r.text();
    return String(r.status) + " " + r.statusText + " " + (r.headers.get("content-type") ?? "none") + " " + body;
  });
  await attempt("stream", async () => {
    const r = await fetch(base + "/stream");
    return await r.text();
  });
  await attempt("missing", async () => {
    const r = await fetch(base + "/missing");
    return String(r.status) + " " + String(r.ok);
  });
  await attempt("post", async () => {
    const r = await fetch(base + "/echo", { method: "POST", body: "привіт", headers: { "content-type": "text/plain;charset=UTF-8" } });
    return await r.text();
  });
  await attempt("runtime header", async () => {
    const r = await fetch(base + "/json", { headers: { connection: "close" } });
    return String(r.status);
  });
  await attempt("too large", async () => {
    const r = await fetch(base + "/large");
    const body = await r.text();
    return String(body.length);
  });
  await attempt("denied", async () => {
    const r = await fetch("http://127.0.0.2:9/x");
    return String(r.status);
  });
}
