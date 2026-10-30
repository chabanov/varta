// Async generators: queued next() requests, yield awaiting its operand,
// return() through finally, and for await closing on break.
async function* countdown(from: number): AsyncGenerator<number, string, undefined> {
  try {
    for (let i = from; i > 0; i--) {
      yield await Promise.resolve(i);
    }
  } finally {
    console.log("countdown closed");
  }
  return "liftoff";
}

export async function main(): Promise<void> {
  const it = countdown(2);
  const first = it.next();
  const second = it.next();
  const third = it.next();
  const results = [await first, await second, await third];
  console.log(JSON.stringify(results));
  let total = 0;
  for await (const n of countdown(5)) {
    total += n;
    if (n === 4) break;
  }
  console.log(String(total));
}
