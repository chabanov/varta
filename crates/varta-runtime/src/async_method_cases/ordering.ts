// Job order of async methods, async arrows and function expressions
// interleaved with promise reactions.
class Counter {
  label: string;
  count: number;
  constructor(label: string) {
    this.label = label;
    this.count = 0;
  }
  async step(tag: string): Promise<number> {
    console.log(`${this.label} step ${tag} start`);
    await null;
    this.count = this.count + 1;
    console.log(`${this.label} step ${tag} resumed ${this.count}`);
    return this.count;
  }
  async twice(tag: string): Promise<number> {
    const a = await this.step(tag + "1");
    const b = await this.step(tag + "2");
    return a + b;
  }
}
class Loud extends Counter {
  async step(tag: string): Promise<number> {
    console.log(`loud ${tag}`);
    const base = await super.step(tag);
    return base * 10;
  }
}
export async function main(): Promise<void> {
  const c = new Counter("c");
  const l = new Loud("l");
  const tick = async (name: string): Promise<void> => {
    console.log(`tick ${name} 1`);
    await null;
    console.log(`tick ${name} 2`);
    await null;
    console.log(`tick ${name} 3`);
  };
  const p1 = c.twice("a");
  const p2 = l.twice("b");
  const p3 = tick("t");
  Promise.resolve(0).then(() => console.log("reaction 1"));
  const nested = async function (depth: number): Promise<number> {
    if (depth === 0) {
      return 0;
    }
    const inner = async (): Promise<number> => depth;
    const own = await inner();
    return own + depth;
  };
  const p4 = nested(3);
  console.log("sync end");
  const r1 = await p1;
  const r2 = await p2;
  await p3;
  const r4 = await p4;
  console.log(`results ${r1} ${r2} ${r4} ${c.count} ${l.count}`);
}
