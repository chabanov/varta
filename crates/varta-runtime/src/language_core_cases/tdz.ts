// Closures capture bindings and hoisted nested functions before their
// initialization; an access before it throws ReferenceError.
function sleepLike(out: string[], abort: boolean): void {
  const timer = (): void => {
    out.push("timer sees " + String(typeof onAbort));
    onAbort();
  };
  const onAbort = (): void => {
    out.push("abort " + String(abort));
  };
  timer();
}
function early(): string {
  const read = (): number => late + 1;
  try {
    read();
    return "no error";
  } catch (e) {
    if (e instanceof Error) return e.name + ": " + e.message;
    return "other";
  }
  const late = 41;
}
function hoisted(n: number): number {
  return even(n) ? 1 : 0;
  function even(k: number): boolean {
    return k === 0 ? true : odd(k - 1);
  }
  function odd(k: number): boolean {
    return k === 0 ? false : even(k - 1);
  }
}
function counter(): number {
  const bump = (): void => {
    count = count + 1;
  };
  let count = 10;
  bump();
  bump();
  return count;
}
function loops(): string {
  let out = "";
  const fs: (() => string)[] = [];
  for (let i = 0; i < 3; i++) {
    fs.push(() => label + String(i));
    const label = "L";
  }
  for (const f of fs) out += f();
  return out;
}
function writeEarly(): string {
  const set = (): void => {
    value = "set";
  };
  try {
    set();
  } catch (e) {
    if (e instanceof Error) return e.name + " " + String(e instanceof Error);
  }
  let value = "init";
  return value;
}
function compoundEarly(): string {
  const bump = (): void => {
    total += 1;
  };
  try {
    bump();
  } catch (e) {
    if (e instanceof Error) return e.message;
  }
  let total = 0;
  return String(total);
}
function nested(): number {
  const outer = (): (() => number) => {
    return (): number => base * 2;
  };
  const get = outer();
  const base = 21;
  return get();
}
function cleanupOrder(): string {
  let out = "";
  const run = (): void => {
    out += "run;";
    cleanup();
  };
  function cleanup(): void {
    out += "cleanup;";
  }
  run();
  return out;
}
async function later(): Promise<string> {
  const read = (): string => text;
  await Promise.resolve();
  const text = "async ok";
  return read();
}
export async function main(): Promise<void> {
  const out: string[] = [];
  sleepLike(out, true);
  console.log(String(out.length) + " " + out[0] + "|" + out[1]);
  console.log(early());
  console.log(String(hoisted(10)) + String(hoisted(7)));
  console.log(String(counter()));
  console.log(loops());
  console.log(writeEarly());
  console.log(compoundEarly());
  console.log(String(nested()));
  console.log(cleanupOrder());
  console.log(await later());
}
