interface Page {
  items: string[];
  next: string | null;
}
async function fetchPage(cursor: string): Promise<Page> {
  await null;                                       // one microtask
  if (cursor === "") {
    return { items: ["a", "b"], next: "p2" };
  }
  return { items: ["c"], next: null };
}
async function* items(): AsyncGenerator<string, void, undefined> {
  let cursor: string | null = "";
  while (cursor !== null) {
    const page = await fetchPage(cursor);
    for (const item of page.items) {
      if (item !== undefined) {
        yield item;
      }
    }
    cursor = page.next;
  }
}
async function* numbered(source: AsyncIterable<string>): AsyncGenerator<string, void, undefined> {
  let n = 0;
  try {
    for await (const item of source) {
      n++;
      yield `${n}. ${item}`;
    }
  } finally {
    console.log(`numbered ${n}`);
  }
}
export async function main(): Promise<void> {
  for await (const line of numbered(items())) {
    console.log(line);                              // 1. a, 2. b, 3. c
  }                                                 // numbered 3
  const it = numbered(items());
  const first = await it.next();
  console.log(JSON.stringify(first));               // {"value":"1. a","done":false}
  const closed = await it.return();                 // numbered 1
  console.log(JSON.stringify(closed));              // {"done":true}
  for await (const n of [Promise.resolve(1), 2]) {
    console.log(String(n));                         // 1, then 2
  }
}
