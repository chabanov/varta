const trace: string[] = [];
interface Sample {
  b: number;
  z: number;
  a: number;
  m: number;
}
function make(): Sample {
  return {
    b: 1,
    get z(): number {
      trace.push("z");
      return 26;
    },
    a: 2,
    get m(): number {
      trace.push("m");
      return 13;
    },
    set m(v: number) {
      trace.push("set m " + String(v));
    },
  };
}
export function main(): void {
  const sample = make();
  console.log(Object.keys(sample).join(","));
  console.log(trace.join(",") + ";");
  console.log(Object.values(sample).join(","));
  console.log(trace.join(","));
  trace.length = 0;
  console.log(Object.entries(sample).map(([key, value]) => key + "=" + String(value)).join(","));
  console.log(trace.join(","));
  const keys: string[] = [];
  for (const key in sample) {
    keys.push(key);
  }
  console.log(keys.join(","));
  trace.length = 0;
  const copy = { ...sample, extra: true };
  console.log(Object.keys(copy).join(",") + " " + JSON.stringify(copy) + " " + trace.join(","));
  trace.length = 0;
  const target = { m: 0, b: 0, a: 0, z: 0 };
  Object.assign(target, sample);
  console.log(JSON.stringify(target) + " " + trace.join(","));
  trace.length = 0;
  const { z, ...others } = sample;
  console.log(String(z) + " " + Object.keys(others).join(",") + " " + JSON.stringify(others) + " " + trace.join(","));
  const direct = {
    get "1"(): number {
      return 1;
    },
    c: 3,
    get "0"(): number {
      return 0;
    },
  };
  console.log(Object.keys(direct).join(","));
}
