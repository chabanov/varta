interface Point {
  y: number;
  x: number;
  label: string;
}
function make(): Point {
  return { y: 2, x: 1, label: "p" };
}
interface Wide {
  y: number;
  x: number;
  label: string;
  z: boolean;
}
export function main(): void {
  const point = make();
  const copy = { ...point };
  console.log(Object.keys(copy).join(",") + " " + JSON.stringify(copy));
  const extended: Wide = { ...point, z: true };
  console.log(Object.keys(extended).join(",") + " " + JSON.stringify(extended));
  const front = { z: false, ...point };
  console.log(Object.keys(front).join(",") + " " + JSON.stringify(front));
  const overridden = { ...point, x: 9 };
  console.log(Object.keys(overridden).join(",") + " " + JSON.stringify(overridden));
  const { y, ...rest } = point;
  console.log(String(y) + " " + Object.keys(rest).join(",") + " " + JSON.stringify(rest));
  const target = { label: "t", x: 0, y: 0 };
  Object.assign(target, point);
  console.log(Object.keys(target).join(",") + " " + JSON.stringify(target));
  const entries = Object.entries({ ...point }).map(([key, value]) => key + "=" + String(value));
  console.log(entries.join(","));
  const numeric = { b: 1, "2": 2, a: 3, "1": 4 };
  const numericCopy = { ...numeric };
  console.log(Object.keys(numericCopy).join(",") + " " + JSON.stringify(numericCopy));
}
