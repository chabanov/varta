// A retrying client: an endless loop that returns from its body, a deferred
// result assigned in `try` with `continue` in `catch`, optional-chain
// narrowing of a field and a per-attempt closure over loop bindings.
class Reply {
  readonly code: number;
  readonly good: boolean;
  constructor(code: number) {
    this.code = code;
    this.good = code < 400;
  }
}
interface Settings { cancel?: AbortSignal; attempts?: number; token?: string }
interface Call { verb: string; safe?: boolean; settings?: Settings }
class Client {
  readonly attempts = 2;
  used = 0;
  readonly script: number[];
  readonly log: string[] = [];
  constructor(script: number[]) {
    this.script = script;
  }
  transmit(call: Call): Reply {
    const code = this.script[this.used];
    this.used++;
    if (code === 0) throw new Error("offline");
    return new Reply(code);
  }
  repeatable(call: Call, token: string | undefined): boolean {
    if (call.verb === "GET") return true;
    return token !== undefined;
  }
  async send(call: Call): Promise<number> {
    const settings = call.settings ?? {};
    const attempts = settings.attempts ?? this.attempts;
    const token = call.safe ? settings.token ?? "auto" : undefined;
    let attempt = 0;
    for (;;) {
      const local = new AbortController();
      const forward = () => local.abort(settings.cancel?.reason);
      settings.cancel?.addEventListener("abort", forward);
      let reply: Reply;
      try {
        if (settings.cancel?.aborted) throw settings.cancel.reason ?? new Error("cancelled");
        reply = this.transmit(call);
      } catch (error) {
        settings.cancel?.removeEventListener("abort", forward);
        this.log.push("attempt " + String(attempt) + " failed");
        if (attempt >= attempts || !this.repeatable(call, token)) {
          throw new Error("gave up after " + String(attempt + 1));
        }
        attempt++;
        continue;
      }
      settings.cancel?.removeEventListener("abort", forward);
      if (reply.good) return reply.code;
      if (attempt >= attempts) throw new Error("status " + String(reply.code));
      attempt++;
    }
  }
}
export async function main(): Promise<void> {
  const client = new Client([0, 503, 200]);
  const code = await client.send({ verb: "GET" });
  console.log(String(code) + " " + String(client.log.length));
  try {
    await new Client([0, 0, 0]).send({ verb: "POST" });
  } catch (e) {
    console.log(e instanceof Error ? e.message : "other");
  }
  const cancel = new AbortController();
  cancel.abort("stop");
  try {
    await new Client([200]).send({ verb: "PUT", safe: true, settings: { cancel: cancel.signal, attempts: 0 } });
  } catch (e) {
    console.log(e instanceof Error ? e.message : "other");
  }
  try {
    await new Client([500, 500, 500]).send({ verb: "GET" });
  } catch (e) {
    console.log(e instanceof Error ? e.message : "other");
  }
}
