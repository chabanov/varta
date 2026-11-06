// Generator methods whose closures capture parameters, locals and `this`.
class Source {
  base = 10;
  *mapped(step: number, label: string): Generator<string> {
    const fmt = (n: number): string => `${label}${n * step + this.base}`;
    for (let i = 0; i < 3; i++) {
      yield fmt(i);
      step += 1;
    }
  }
  *[Symbol.iterator](): Generator<number> {
    const extra = (): number => this.base;
    yield extra();
    this.base += 1;
    yield extra();
  }
}
export function main(): void {
  const s = new Source();
  console.log([...s.mapped(2, '#')].join(' '));
  console.log([...s, ...s].join(' '));
  const g = s.mapped(1, '>');
  console.log(`${g.next().value} ${g.return(undefined).done} ${g.next().done}`);
}
