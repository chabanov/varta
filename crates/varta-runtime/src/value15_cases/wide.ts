// A 60-field interface (the pilot's widest record): presence words beyond
// 32 slots, optional fields, deletion, enumeration, JSON and collection.
interface Agent {
  f00: string;
  f01: number;
  f02: boolean;
  f03?: string | null;
  f04: string[];
  f05: string;
  f06: number;
  f07: boolean;
  f08: string | null;
  f09: string[];
  f10?: string;
  f11: number;
  f12: boolean;
  f13: string | null;
  f14: string[];
  f15: string;
  f16: number;
  f17?: boolean;
  f18: string | null;
  f19: string[];
  f20: string;
  f21: number;
  f22: boolean;
  f23: string | null;
  f24?: string[];
  f25: string;
  f26: number;
  f27: boolean;
  f28: string | null;
  f29: string[];
  f30: string;
  f31?: number;
  f32: boolean;
  f33: string | null;
  f34: string[];
  f35: string;
  f36: number;
  f37: boolean;
  f38?: string | null;
  f39: string[];
  f40: string;
  f41: number;
  f42: boolean;
  f43: string | null;
  f44: string[];
  f45?: string;
  f46: number;
  f47: boolean;
  f48: string | null;
  f49: string[];
  f50: string;
  f51: number;
  f52?: boolean;
  f53: string | null;
  f54: string[];
  f55: string;
  f56: number;
  f57: boolean;
  f58: string | null;
  f59?: string[];
  "7": number;
}
function make(salt: string): Agent {
  let garbage = "";
  for (let i = 0; i < 40; i++) {
    garbage = garbage + salt + String(i);
  }
  const agent: Agent = { f00: "s0x", f01: 1.5, f02: false, f04: ["a4", "bx"], f05: "s5x", f06: 6.5, f07: true, f08: "n8", f09: ["a9", "bx"], f11: 11.5, f12: false, f13: null, f14: ["a14", "bx"], f15: "s15x", f16: 16.5, f18: "n18", f19: ["a19", "bx"], f20: "s20x", f21: 21.5, f22: false, f23: null, f25: "s25x", f26: 26.5, f27: true, f28: "n28", f29: ["a29", "bx"], f30: "s30x", f32: false, f33: null, f34: ["a34", "bx"], f35: "s35x", f36: 36.5, f37: true, f39: ["a39", "bx"], f40: "s40x", f41: 41.5, f42: false, f43: null, f44: ["a44", "bx"], f46: 46.5, f47: true, f48: "n48", f49: ["a49", "bx"], f50: "s50x", f51: 51.5, f53: null, f54: ["a54", "bx"], f55: "s55x", f56: 56.5, f57: true, f58: "n58", "7": 7 };
  agent.f01 = garbage.length;
  return agent;
}
function mutate(first: Agent): void {
  console.log(String(Object.keys(first).length) + " " + String(first.f45 === undefined));
  const late: string[] = ["late"];
  first.f59 = late;
  const opt = "opt";
  first.f45 = opt;
  first.f03 = "three";
  console.log(String(Object.keys(first).length) + " " + String(Object.hasOwn(first, "f45")));
  console.log(JSON.stringify(first));
  delete first.f45;
  delete first.f03;
  const keys = Object.keys(first);
  console.log(String(keys.length) + " " + keys[0] + " " + keys[1] + " " + keys[keys.length - 1]);
}
function split(last: Agent): void {
  const { f00, f59, ...rest } = last;
  console.log(f00 + " " + String(f59 === undefined) + " " + String(Object.keys(rest).length) + " " + String(rest.f57));
}
export function main(): void {
  const agents: Agent[] = [];
  for (let i = 0; i < 6; i++) {
    agents.push(make(String(i)));
  }
  const first = agents[0];
  const last = agents[5];
  if (first === undefined || last === undefined) {
    return;
  }
  mutate(first);
  console.log(String(last.f01) + " " + String(last.f58) + " " + String(last.f39.length));
  split(last);
  const text = JSON.stringify(last);
  console.log(String(text.length));
  const echoed = () => last.f55;
  console.log(echoed());
}
