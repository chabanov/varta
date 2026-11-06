// Literal inference and widening as tsc does it.
const mode = "dark";
type Mode = typeof mode;

function describe(): string {
  const size = "sm";
  let copy = size;
  copy = "lg";
  const count = 3;
  const fixed: 3 = count;
  const flag = true;
  const yes: true = flag;
  const alias = size;
  const same: "sm" = alias;
  const pair = ["a", 1] as const;
  const first: "a" = pair[0];
  const point = { x: 1, label: "p" } as const;
  const label: "p" = point.label;
  const values = [size, "md"];
  values.push("xl");
  return `${size} ${copy} ${fixed} ${yes} ${same} ${first} ${label} ${values.join(",")}`;
}

export function main(): void {
  const current: Mode = mode;
  console.log(`${current} ${describe()}`);
  let loose = mode;
  loose = "light";
  console.log(loose);
}
