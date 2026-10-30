// Views stay valid across suspensions and collections: class instances and
// wider records are read and called through narrower interface types while
// every round allocates garbage between jobs.
interface Source {
  label: string;
  next(): number;
}
class Ticker {
  label: string;
  #n = 0;
  constructor(label: string) {
    this.label = label;
  }
  next(): number {
    this.#n += 1;
    return this.#n;
  }
}
interface Entry {
  name: string;
  parts: string[];
}
interface Named {
  name: string;
}

async function tick(): Promise<void> {}

async function drive(sources: Source[], named: Named[]): Promise<string> {
  let text = "";
  for (let round = 0; round < 3; round++) {
    await tick();
    for (const s of sources) text += `${s.label}${s.next()} `;
    for (const n of named) text += `${n.name.length},`;
    const junk: string[] = [];
    for (let i = 0; i < 20; i++) junk.push(`x${i}${round}`);
    text += `${junk.length};`;
  }
  return text;
}

export async function main(): Promise<void> {
  const t = new Ticker("t");
  const sources: Source[] = [
    t,
    {
      label: "o",
      next(): number {
        return 7;
      },
    },
  ];
  const named: Named[] = [];
  for (let i = 0; i < 5; i++) {
    const e: Entry = { name: `entry${i}`, parts: [`p${i}`] };
    named.push(e);
  }
  console.log(await drive(sources, named));
  console.log(`${JSON.stringify(named[2])} ${t.next()}`);
}
