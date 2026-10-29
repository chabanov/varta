// The examples of the v33 language guide section.
function* pages(limit: number): Generator<string, number, number> {
  let page = 0;
  try {
    while (page < limit) {
      const skip = yield `page ${page}`;
      page += 1 + skip;
    }
  } finally {
    console.log("closed");
  }
  return page;
}
function* count(n: number): Generator<number> {
  try {
    for (let i = 0; i < n; i++) yield i;
  } finally {
    console.log("cleanup");
  }
}
export function main(): void {
  const g = pages(5);
  console.log(String(g.next(0).value));
  console.log(String(g.next(1).value));
  const r = g.return(9);
  console.log(`${String(r.done)} ${String(r.value)}`);
  console.log(String(g.next(0).done));
  for (const i of count(10)) {
    if (i === 2) break;
    console.log(String(i));
  }
}
