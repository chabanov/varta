interface Counter { count: number; inc(): Counter; read(): number }
export function main(): void {
  const o = {
    n: 2,
    dbl(): number { return this.n * 2; },
    scale(k: number): number { return this.n * k; },
  };
  console.log(String(o.dbl()) + " " + String(o.scale(5)));
  o.n = 7;
  console.log(String(o.dbl()));
  const c: Counter = {
    count: 0,
    inc() { this.count++; return this; },
    read() { return this.count; },
  };
  console.log(String(c.inc().inc().read()));
  const size = 2;
  const shorthand = { c, size };
  console.log(String(shorthand.c.count + shorthand.size));
  const label = "x";
  const tagged = { label, show(): string { return `${this.label}!`; }, later(): () => string { return () => this.label + "?"; } };
  console.log(tagged.show() + tagged.later()());
}
