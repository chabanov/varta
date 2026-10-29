// Rejections: one-time resolution, executor exceptions, catch/finally
// chaining, the self-resolution TypeError and try/catch/finally across await.
function say(text: string): void {
  console.log(text);
}
function describe(reason: Error | string | number): string {
  if (reason instanceof Error) {
    return reason.name + ": " + reason.message;
  }
  return "value " + String(reason);
}
async function fails(text: string): Promise<number> {
  await null;
  throw new RangeError(text);
}
async function guarded(): Promise<string> {
  try {
    say("guarded try");
    const value = await fails("inner");
    return "unreachable " + String(value);
  } catch (error) {
    if (error instanceof Error) {
      say("guarded caught " + error.message);
    }
    return "recovered";
  } finally {
    say("guarded finally");
  }
}
async function cleanup(): Promise<number> {
  try {
    await 1;
    return 10;
  } finally {
    say("cleanup finally");
    await 2;
    say("cleanup after await in finally");
  }
}
export function main(): void {
  const once = new Promise<number>((resolve, reject) => {
    resolve(1);
    resolve(2);
    reject(new Error("ignored"));
  });
  once.then((v) => say("once " + String(v)));
  const thrown = new Promise<number>(() => {
    throw new TypeError("executor failed");
  });
  thrown.catch((e) => {
    if (e instanceof Error) {
      say("executor " + e.name + " " + e.message);
    }
    return 0;
  });
  const settledThenThrow = new Promise<string>((resolve) => {
    resolve("kept");
    throw new Error("after resolve");
  });
  settledThenThrow.then((v) => say("settled " + v));
  Promise.reject<number>("plain")
    .then((v) => v + 1)
    .catch((e) => {
      if (typeof e === "string") {
        say("passed through " + e);
      }
      return -1;
    })
    .then((v) => say("after catch " + String(v)));
  Promise.resolve(5)
    .finally(() => say("finally sees no value"))
    .then((v) => say("finally passes " + String(v)));
  Promise.reject<number>(new Error("kept reason"))
    .finally(() => say("finally on rejection"))
    .catch((e) => {
      if (e instanceof Error) {
        say("finally rethrows " + e.message);
      }
      return 0;
    });
  Promise.resolve(6)
    .finally(() => {
      throw new Error("finally overrides");
    })
    .catch((e) => {
      if (e instanceof Error) {
        say(e.message);
      }
      return 0;
    });
  let cyclic: Promise<number> = Promise.resolve(0);
  cyclic = Promise.resolve(1).then(() => cyclic);
  cyclic.catch((e) => {
    if (e instanceof TypeError) {
      say("cycle " + e.name + ": " + e.message);
    }
    return 0;
  });
  guarded().then((text) => say("guarded " + text));
  cleanup().then((v) => say("cleanup " + String(v)));
  fails("direct").catch((e) => {
    if (e instanceof RangeError) {
      say("direct " + e.name + " " + e.message);
    }
    return 0;
  });
  say(describe("sync"));
}
