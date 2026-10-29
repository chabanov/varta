// Reference values across suspension, switch cases, closures, re-entry and
// generic generators.
function join(items: string[], sep: string): string {
  let text = "";
  for (let i = 0; i < items.length; i++) {
    const item = items[i];
    if (item !== undefined) {
      text = i === 0 ? item : text + sep + item;
    }
  }
  return text;
}
interface Page {
  items: string[];
  next: number;
}
function* pages(count: number): Generator<Page, string, number> {
  let cursor = 0;
  let text = "";
  while (cursor < count) {
    for (let i = 0; i < 16; i++) {
      text = text + "garbage-" + String(i);
    }
    const page: Page = { items: ["p" + String(cursor), "q" + String(cursor)], next: cursor + 1 };
    const skip = yield page;
    cursor = cursor + 1 + skip;
  }
  return "pages " + String(text.length);
}
function* steps(kind: number): Generator<string> {
  switch (kind) {
    case 1:
      yield "one";
    case 2:
      yield "two";
      break;
    default:
      yield "other";
  }
  yield "end";
}
function* counters(): Generator<number> {
  let total = 0;
  const add = (n: number): number => {
    total = total + n;
    return total;
  };
  yield add(1);
  yield add(10);
  yield total;
}
function* repeat<T>(value: T, times: number): Generator<T> {
  for (let i = 0; i < times; i++) {
    yield value;
  }
}
function* reenter(cb: () => string): Generator<string> {
  yield "a";
  yield cb();
}
export function main(): void {
  const p = pages(5);
  let r = p.next(0);
  const seen: string[] = [];
  while (!r.done) {
    const value = r.value;
    if (value !== undefined && typeof value !== "string") {
      seen.push(join(value.items, "+"));
    }
    r = p.next(1);
  }
  const final = r.value;
  console.log(join(seen, ",") + " " + (typeof final === "string" ? final : "?"));
  for (let k = 1; k <= 3; k++) {
    const out: string[] = [];
    for (const s of steps(k)) {
      out.push(s);
    }
    console.log("switch " + String(k) + " " + join(out, ","));
  }
  const counted: string[] = [];
  for (const n of counters()) {
    counted.push(String(n));
  }
  console.log("closures " + join(counted, ","));
  const words: string[] = [];
  for (const w of repeat("hi", 2)) {
    words.push(w);
  }
  let flags = 0;
  for (const b of repeat(true, 3)) {
    if (b) {
      flags++;
    }
  }
  console.log("generic " + join(words, ",") + " " + String(flags));
  let hook = (): string => "unset";
  const it = reenter((): string => hook());
  hook = (): string => {
    try {
      it.next();
      return "no error";
    } catch (e) {
      return e instanceof TypeError ? "TypeError " + e.message : "other";
    }
  };
  console.log(String(it.next().value));
  console.log(String(it.next().value));
  console.log(String(it.next().done));
}
