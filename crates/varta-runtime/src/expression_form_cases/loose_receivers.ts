interface Counter {
  count: number;
  label: string;
  bump(): number;
  name(): string;
}
interface Other {
  count: number;
  bump: () => number;
}
export function main(): void {
  const counter: Counter = {
    count: 1,
    label: "c",
    bump(): number {
      this.count += 1;
      return this.count;
    },
    name(): string {
      return this.label;
    },
  };
  // A spread copy carries the methods: `this` is the copy.
  const copy = { ...counter, extra: true };
  console.log(String(copy.bump()) + " " + String(counter.count));
  console.log(JSON.stringify(copy));
  // A receiver of another layout that has the members the method reads.
  const other: Other = { count: 10, bump: counter.bump };
  console.log(String(other.bump()) + " " + String(other.count));
}
