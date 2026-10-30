class Body {
  text: string;
  constructor(text: string) {
    this.text = text;
  }
}
class Reply {
  #body: Body | null;
  reads = 0;
  constructor(body: Body | null) {
    this.#body = body;
  }
  get body(): Body | null {
    this.reads += 1;
    return this.#body;
  }
  drop(): void {
    this.#body = null;
  }
}
interface Options {
  signal?: { aborted: boolean; reason: string };
  label: string;
}
function note(text: string): void {
  console.log(`note ${text}`);
}
function consume(body: Body): string {
  return body.text;
}
function read(reply: Reply): string {
  if (!reply.body) {
    return "none";
  }
  note("connected");
  return consume(reply.body);
}
function abortReason(options: Options): string {
  if (options.signal) {
    note(options.label);
    if (options.signal.aborted) return options.signal.reason;
    return "live";
  }
  return "no signal";
}
export function main(): void {
  const reply = new Reply(new Body("hello"));
  console.log(`${read(reply)} ${reply.reads}`);
  console.log(read(new Reply(null)));
  console.log(abortReason({ label: "a", signal: { aborted: true, reason: "stop" } }));
  console.log(abortReason({ label: "b" }));
  const sneaky = new Reply(new Body("x"));
  try {
    if (sneaky.body) {
      sneaky.drop();
      console.log(consume(sneaky.body));
    }
  } catch (error) {
    console.log(error instanceof TypeError ? "TypeError" : "other");
  }
}
