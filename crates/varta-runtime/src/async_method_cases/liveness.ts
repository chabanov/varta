// Async closures and methods under v35 liveness frames: captured cells are
// shared slots of the body frame, assignments after an await write through
// the captured cell, and outer frames keep the cells their closures share.
async function tick(label: string): Promise<string> {
  await null;
  return `${label}!`;
}
class Counter {
  label: string;
  total: number;
  constructor(label: string) {
    this.label = label;
    this.total = 0;
  }
  async run(times: number): Promise<string> {
    let last = "";
    let seen = 0;
    const step = async (n: number): Promise<void> => {
      const value = await tick(`${this.label}${n}`);
      last = value;
      seen = seen + 1;
      this.total = this.total + n;
    };
    for (let i = 1; i <= times; i++) {
      await step(i);
    }
    return `${last} seen=${seen} total=${this.total}`;
  }
}
export async function main(): Promise<void> {
  let shared = "start";
  let hits = 0;
  const assignOnly = async (): Promise<void> => {
    await null;
    shared = "assigned";
  };
  const append = async (suffix: string): Promise<number> => {
    const before = shared;
    await tick(suffix);
    shared = `${before}+${suffix}`;
    hits = hits + 1;
    return hits;
  };
  const readLater = async (): Promise<string> => {
    await null;
    await null;
    return `${shared}/${hits}`;
  };
  await assignOnly();
  console.log(shared);
  const first = append("a");
  const second = append("b");
  const a = await first;
  const b = await second;
  console.log(`${a} ${b} ${shared}`);
  console.log(await readLater());
  const counter = new Counter("c");
  console.log(await counter.run(3));
  console.log(await counter.run(2));
  const pending = append("x");
  const results = await Promise.all([readLater(), counter.run(1)]);
  const x = await pending;
  console.log(`${x} ${results[0]} ${results[1]}`);
}
