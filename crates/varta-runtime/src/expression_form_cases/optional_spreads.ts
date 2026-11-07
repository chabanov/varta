interface P {
  a: number;
}
interface PO {
  a?: number;
}
interface PB {
  a?: number;
  b: number;
}
function spread(p: P | undefined): PO {
  return { ...p };
}
function spreadWith(p: P | undefined): PB {
  return { ...p, b: 1 };
}
export function main(): void {
  console.log(JSON.stringify(spread(undefined)));
  console.log(JSON.stringify(spread({ a: 2 })));
  console.log(JSON.stringify(spreadWith(undefined)));
  console.log(JSON.stringify(spreadWith({ a: 3 })));
  const r = spread(undefined);
  console.log(String(r.a));
}
