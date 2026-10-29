// An async function builds a dictionary of wide records (40 fields) across
// awaits; the dictionary, its vectors and every record stay frame-rooted while
// garbage churn forces collections between the suspensions.
interface Wide {
  id: string;
  f0?: number; f1?: number; f2?: number; f3?: number; f4?: number;
  f5?: number; f6?: number; f7?: number; f8?: number; f9?: number;
  f10?: number; f11?: number; f12?: number; f13?: number; f14?: number;
  f15?: number; f16?: number; f17?: number; f18?: number; f19?: number;
  f20?: number; f21?: number; f22?: number; f23?: number; f24?: number;
  f25?: number; f26?: number; f27?: number; f28?: number; f29?: number;
  f30?: number; f31?: number; f32?: number; f33?: number; f34?: number;
  f35?: number; f36?: number; f37?: number; f38?: number;
  tags: Record<string, string>;
}
function churn(): number {
  // Each call allocates about 8 KB of garbage strings; together they force
  // collections while the dictionary and its records are frame-rooted.
  let text = "x";
  for (let i = 0; i < 11; i++) {
    text = text + text;
  }
  return text.length;
}
async function fetchWide(n: number): Promise<Wide> {
  await null;
  churn();
  const tags: Record<string, string> = {};
  tags["kind"] = n % 2 === 0 ? "even" : "odd";
  const wide: Wide = { id: `w${n}`, tags: tags };
  wide.f38 = n * 10;
  wide.f0 = n;
  return wide;
}
async function load(count: number): Promise<Record<string, Wide>> {
  const out: Record<string, Wide> = {};
  for (let n = 0; n < count; n++) {
    const wide = await fetchWide(n);
    churn();
    out[`k${n}`] = wide;
  }
  // `await` inside a dictionary write is an intrinsic value block: await first.
  const last = await fetchWide(9);
  out["0"] = last;
  return out;
}
export async function main(): Promise<void> {
  const pending = load(3);
  console.log("started");
  const loaded = await pending;
  churn();
  console.log(JSON.stringify(Object.keys(loaded)));
  const first = loaded["k1"];
  if (first !== undefined) {
    console.log(`${first.id} ${first.f38} ${first.f0} ${first.tags["kind"]}`);
  }
  console.log(JSON.stringify(loaded["0"]));
  console.log(`${"k2" in loaded} ${"k9" in loaded} ${Object.hasOwn(loaded, "0")}`);
}
