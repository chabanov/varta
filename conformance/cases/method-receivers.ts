interface Counter {
  count: number;
  step: number;
  bump(): number;
  describe(): string;
}
function makeCounter(step: number): Counter {
  return {
    count: 0,
    step,
    bump(): number {
      this.count += this.step;
      return this.count;
    },
    describe(): string {
      return "count=" + String(this.count) + " step=" + String(this.step);
    },
  };
}
function caught(run: () => string): string {
  try {
    return run();
  } catch (error) {
    return error instanceof TypeError ? "TypeError" : "other";
  }
}
export async function main(): Promise<void> {
  const a = makeCounter(2);
  a.bump();
  a.bump();
  console.log(a.describe());
  // A spread copy has the same shape: its methods see the copy.
  const b = { ...a };
  b.bump();
  console.log(a.describe() + " | " + b.describe());
  // Methods moved between objects of the same type see their new receiver.
  const c = makeCounter(10);
  c.bump = a.bump;
  c.bump();
  console.log(a.describe() + " | " + c.describe());
  // An extracted method sees `undefined`.
  const loose = a.describe;
  console.log(caught(() => loose()));
  // Own keys of an object with methods enumerate as in JavaScript.
  console.log(Object.keys(a).join(","));
  const point = {
    x: 3,
    y: 4,
    norm(): number {
      return Math.sqrt(this.x * this.x + this.y * this.y);
    },
    scaled(k: number): number {
      return this.norm() * k;
    },
  };
  console.log(String(point.scaled(2)));
  const moved = { ...point, x: 6, y: 8 };
  console.log(String(moved.norm()) + " " + String(point.norm()));
  const list: Counter[] = [makeCounter(1), makeCounter(5)];
  for (const item of list) {
    item.bump();
  }
  console.log(list.map((item) => item.describe()).join(", "));
  const account = {
    balance: 10,
    async add(n: number): Promise<number> {
      const before = this.balance;
      await Promise.resolve();
      this.balance = before + n;
      return this.balance;
    },
  };
  const copy = { ...account };
  const sums = await Promise.all([account.add(5), copy.add(1)]);
  console.log(sums.join(" ") + " " + String(account.balance) + " " + String(copy.balance));
  // Through a structural view (a type with fewer members) the method still
  // sees the object.
  const widget = {
    name: "widget",
    size: 3,
    label(): string {
      return this.name + "#" + String(this.size);
    },
  };
  console.log(labelOf(widget));
}
function labelOf(item: { label(): string }): string {
  return item.label();
}
