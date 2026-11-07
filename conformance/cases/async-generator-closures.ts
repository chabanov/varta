interface Feed {
  prefix: string;
  lines(count: number): AsyncGenerator<string, void, unknown>;
}
function tick(value: number): Promise<number> {
  return Promise.resolve(value);
}
function source(offset: number): () => AsyncGenerator<number, string, unknown> {
  let started = 0;
  return async function* (): AsyncGenerator<number, string, unknown> {
    started += 1;
    for (let i = 0; i < 2; i++) yield (await tick(i)) + offset + started;
    return "end " + started;
  };
}
export async function main(): Promise<void> {
  // An `async function*` expression closes over its environment.
  const s = source(10);
  const seen: number[] = [];
  for await (const value of s()) seen.push(value);
  for await (const value of s()) seen.push(value);
  console.log(seen.join(","));
  const g = s();
  console.log(JSON.stringify(await g.next()));
  console.log(JSON.stringify(await g.next()));
  console.log(JSON.stringify(await g.next()));
  // Async generator methods of object literals, with and without `this`.
  const feed: Feed = {
    prefix: "#",
    async *lines(count: number): AsyncGenerator<string, void, unknown> {
      for (let i = 0; i < count; i++) yield this.prefix + (await tick(i));
    },
  };
  const out: string[] = [];
  for await (const line of feed.lines(3)) out.push(line);
  console.log(out.join(" "));
  let calls = 0;
  const plain = {
    async *numbers(): AsyncGenerator<number, void, unknown> {
      calls += 1;
      yield calls;
      yield await tick(calls * 10);
    },
  };
  const numbers: number[] = [];
  for await (const n of plain.numbers()) numbers.push(n);
  console.log(numbers.join(",") + " " + calls);
}
