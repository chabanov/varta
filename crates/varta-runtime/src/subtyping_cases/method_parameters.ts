// A source method or function may declare more parameters than the target
// when every extra one is optional, has a default or is the rest parameter:
// calls through the target pass `undefined` (the default runs in the callee)
// or an empty rest array.
interface Options {
  timeout?: number;
}
interface Api {
  send(id: string): string;
}
class Impl {
  send(id: string, options?: Options): string {
    return "ok " + id + (options === undefined ? "" : " t=" + options.timeout);
  }
}

// `this` inside the adapted method, defaults and rest parameters.
interface Port {
  name: string;
  call(path: string): string;
  count(n: number): number;
  join(a: string): string;
  ping(): void;
}
class Client {
  name = "client";
  calls = 0;
  call(path: string, options?: Options, retries: number = 2): string {
    this.calls += 1;
    const timeout = options?.timeout ?? -1;
    return `${this.name}${path} t=${timeout} r=${retries} #${this.calls}`;
  }
  count(n: number, step = 10): number {
    return n + step;
  }
  join(a: string, ...rest: string[]): string {
    return a + ":" + rest.length + ":" + rest.join("|");
  }
  ping(extra?: string): number {
    this.calls += 100;
    return extra === undefined ? 1 : 2;
  }
}

// Async methods through a view.
interface Loader {
  load(key: string): Promise<string>;
}
class Store {
  prefix = "v";
  async load(key: string, fresh?: boolean): Promise<string> {
    await Promise.resolve();
    return this.prefix + ":" + key + ":" + (fresh === undefined ? "cached" : String(fresh));
  }
}

// Function fields of records and classes reached through views.
interface Handler {
  run: (x: number) => string;
}
class Holder {
  run: (x: number, scale?: number) => string = (x: number, scale?: number): string =>
    "h" + x * (scale ?? 3);
}

// Plain function values.
function wide(a: string, b?: number): string {
  return a + (b === undefined ? "?" : String(b));
}
function defaults(a: string, b: string = "dflt"): string {
  return a + "+" + b;
}
function tail(a: string, ...more: number[]): string {
  return a + more.length;
}
function wider(value: string | number): string {
  return typeof value === "string" ? "s:" + value : "n:" + value;
}
type Unary = (a: string) => string;

function apply(f: Unary, x: string): string {
  return f(x);
}
function maybe(f: Unary | undefined): string {
  return f === undefined ? "none" : f("m");
}

// Async generator methods, `implements` and async function values.
interface Lines {
  split(text: string): AsyncGenerator<string, void, undefined>;
}
class Splitter {
  sep = ",";
  async *split(text: string, limit?: number): AsyncGenerator<string, void, undefined> {
    let n = 0;
    for (const part of text.split(this.sep)) {
      if (limit !== undefined && n >= limit) return;
      n++;
      yield part;
    }
  }
}
interface Numbers {
  first(n: number): number;
}
class Steps implements Numbers {
  base = 5;
  first(n: number, offset: number = 1): number {
    return this.base + n + offset;
  }
}
async function fetchOne(id: string, options?: Options): Promise<string> {
  await Promise.resolve();
  return id + "/" + (options?.timeout ?? 0);
}
type Fetch = (id: string) => Promise<string>;

// Adapters and the objects they reach held only by a suspended frame.
async function later(fs: Unary[], fetch: Fetch, port: Port, lines: Lines): Promise<string> {
  await Promise.resolve();
  let out = "";
  for (const f of fs) out += f("g") + ";";
  for await (const line of lines.split("a,b,c")) out += line;
  const fetched = await fetch("f");
  return out + " " + fetched + " " + port.call("/late");
}

export function main(): void {
  const a: Api = new Impl();
  console.log(a.send("x"));
  // One view, a class method source and a function field source.
  const record = { send: (id: string, retries?: number): string => "rec " + id + (retries ?? 0) };
  const apis: Api[] = [a, record, new Impl()];
  let sent = "";
  for (const api of apis) sent += api.send("y") + ";";
  console.log(sent);
  // Literals with extra parameters in a narrower context.
  const literal: Api = {
    send(id: string, options?: Options): string {
      return "lit " + id + (options?.timeout ?? "");
    },
  };
  const arrow: Unary = (s: string, n: number = 4, ...more: string[]): string =>
    s + n + more.length;
  const expression: Unary = function (s: string, flag?: boolean): string {
    return s + (flag === undefined ? "-" : "+");
  };
  console.log(`${literal.send("z")} ${arrow("a")} ${expression("e")}`);

  const client = new Client();
  const port: Port = client;
  console.log(port.call("/a"));
  console.log(port.call("/b"));
  console.log(`${port.count(1)} ${port.join("j")} ${port.name}`);
  port.ping();
  console.log(String(client.calls));

  const loader: Loader = new Store();
  loader.load("k").then((value) => console.log(value));

  const runner = { run: (x: number, suffix?: string): string => "r" + x + (suffix ?? "") };
  const viewed: Handler = runner;
  const holder: Handler = new Holder();
  const run = viewed.run;
  console.log(`${viewed.run(1)} ${holder.run(2)} ${run(4)}`);

  const fs: Unary[] = [wide, defaults, tail, wider];
  let out = "";
  for (const f of fs) out += f("p") + ",";
  console.log(out);
  let captured = 0;
  const closure = (s: string, n?: number): string => {
    captured += n ?? 1;
    return s + captured;
  };
  console.log(`${apply(closure, "c")} ${apply(closure, "d")} ${maybe(wide)} ${maybe(undefined)}`);

  const numbers: Numbers = new Steps();
  console.log(String(numbers.first(1)));
  const lines: Lines = new Splitter();
  later([closure, (s: string): string => s + "!", wide], fetchOne, new Client(), lines).then(
    (v) => console.log(v),
  );
}
