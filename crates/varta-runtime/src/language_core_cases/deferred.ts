// Deferred initialization with definite assignment.
interface Res { ok: boolean; body: string }
function fetchish(n: number): Res {
  if (n < 0) throw new Error("negative");
  return { ok: n > 0, body: "b" + String(n) };
}
function viaTry(n: number): string {
  let response: Res;
  try {
    response = fetchish(n);
  } catch (error) {
    return "caught " + (error instanceof Error ? error.message : "?");
  }
  return response.body + " " + String(response.ok);
}
function viaSwitch(k: number): string {
  let s: string;
  switch (k) {
    case 1:
      s = "one";
      break;
    case 2:
    case 3:
      s = "few";
      break;
    default:
      s = "many";
  }
  return s;
}
function viaIf(flag: boolean): number {
  let x: number;
  if (flag) {
    x = 1;
  } else if (!flag) {
    x = 2;
  } else {
    throw new Error("unreachable");
  }
  x += 10;
  x++;
  return x;
}
function viaLoop(): number {
  let total: number;
  total = 0;
  for (let i = 0; i < 4; i++) {
    let step: number;
    if (i % 2 === 0) step = i;
    else step = -1;
    total += step;
  }
  const read = (): number => total;
  return read();
}
function viaFinally(): string {
  let done: string;
  try {
    done = "body";
  } finally {
    done = "finally";
  }
  return done;
}
export function main(): void {
  console.log(viaTry(2));
  console.log(viaTry(-1));
  console.log(viaSwitch(1) + viaSwitch(3) + viaSwitch(9));
  console.log(String(viaIf(true)) + " " + String(viaIf(false)));
  console.log(String(viaLoop()));
  console.log(viaFinally());
}
