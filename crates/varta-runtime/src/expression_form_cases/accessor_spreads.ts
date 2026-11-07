const trace: string[] = [];
interface Base {
  b: number;
  a: string;
}
interface Wide {
  b: number;
  a: string;
  readonly c: number;
}
interface Over {
  readonly a: string;
  b: number;
}
interface Counted {
  readonly n: number;
  b: number;
}
export function main(): void {
  const base: Base = { b: 1, a: "x" };
  const wide: Wide = {
    ...base,
    get c(): number {
      trace.push("c");
      return this.b + 1;
    },
  };
  console.log(Object.keys(wide).join(","));
  console.log(JSON.stringify(wide));
  wide.b = 5;
  console.log(String(wide.c));
  // An accessor after a spread takes the spread property's place.
  const over: Over = {
    ...base,
    get a(): string {
      return "getter";
    },
  };
  console.log(Object.keys(over).join(",") + " " + over.a);
  // A spread after an accessor makes the property data again.
  const source: Counted = {
    b: 7,
    get n(): number {
      trace.push("n");
      return 3;
    },
  };
  const counted: Counted = {
    get b(): number {
      return 0;
    },
    ...source,
  };
  console.log(Object.keys(counted).join(",") + " " + JSON.stringify(counted));
  counted.b = 9;
  console.log(String(counted.b));
  console.log(trace.join(","));
}
