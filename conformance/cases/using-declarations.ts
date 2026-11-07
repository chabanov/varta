class Res implements Disposable {
  name: string;
  constructor(name: string) { this.name = name; console.log("open " + name); }
  [Symbol.dispose](): void { console.log("close " + this.name); }
}
class Conn implements AsyncDisposable {
  async [Symbol.asyncDispose](): Promise<void> { await null; console.log("async closed"); }
}
function early(flag: boolean): string {
  using r = new Res("early");
  if (flag) { return "returned"; }
  using s = new Res("late");
  return "end";
}
export async function main(): Promise<void> {
  console.log(early(true));
  console.log(early(false));
  for (const n of [1, 2]) {
    using loop = new Res("loop" + String(n));
    if (n === 1) { continue; }
    console.log("body " + String(n));
  }
  {
    await using c = new Conn();
    using r = new Res("inner");
    console.log("work");
  }
  try {
    using a = new Res("a"), b = new Res("b");
    throw new Error("boom");
  } catch (e) {
    console.log("caught " + (e as Error).message);
  }
}
