// The executor runs synchronously; resolution happens once; an executor
// exception rejects; resolving a promise with itself rejects with a TypeError;
// finally passes the value through and a throwing finally overrides it.
export function main(): void {
  const once = new Promise<string>((resolve, reject) => {
    console.log("executor");
    resolve("first");
    resolve("second");
    reject(new Error("ignored"));
  });
  once.then((v) => console.log("once " + v));
  new Promise<number>(() => {
    throw new RangeError("executor failed");
  }).catch((e) => {
    if (e instanceof RangeError) {
      console.log("caught " + e.message);
    }
    return 0;
  });
  let cyclic: Promise<number> = Promise.resolve(0);
  cyclic = Promise.resolve(1).then(() => cyclic);
  cyclic.catch((e) => {
    if (e instanceof TypeError) {
      console.log(e.name + ": " + e.message);
    }
    return 0;
  });
  Promise.resolve(5)
    .finally(() => console.log("finally"))
    .then((v) => console.log("kept " + String(v)));
  Promise.resolve(6)
    .finally(() => {
      throw new Error("overridden");
    })
    .catch((e) => {
      if (e instanceof Error) {
        console.log(e.message);
      }
      return 0;
    });
  console.log("sync end");
}
