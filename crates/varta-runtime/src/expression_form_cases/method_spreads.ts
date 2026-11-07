interface Base {
  count: number;
  label: string;
}
interface Counter {
  count: number;
  label: string;
  bump(): number;
}
export function main(): void {
  const base: Base = { label: "b", count: 1 };
  const counter: Counter = {
    ...base,
    bump(): number {
      this.count += 10;
      return this.count;
    },
  };
  console.log(String(counter.bump()) + " " + String(base.count));
  console.log(Object.keys(counter).join(","));
  const plain = {
    ...base,
    count: 5,
    twice(): number {
      return this.count * 2;
    },
  };
  console.log(String(plain.twice()) + " " + Object.keys(plain).join(","));
}
