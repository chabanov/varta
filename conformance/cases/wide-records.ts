// A closed record wider than 32 fields: presence, writes, deletion of an
// optional field beyond slot 32 and JSON serialization.
interface Wide {
  f0: number;
  f1: number;
  f2: number;
  f3: number;
  f4: number;
  f5: number;
  f6: number;
  f7: number;
  f8: number;
  f9: number;
  f10: number;
  f11: number;
  f12: number;
  f13: number;
  f14: number;
  f15: number;
  f16: number;
  f17: number;
  f18: number;
  f19: number;
  f20: number;
  f21: number;
  f22: number;
  f23: number;
  f24: number;
  f25: number;
  f26: number;
  f27: number;
  f28: number;
  f29: number;
  f30: number;
  f31: number;
  f32: number;
  f33: number;
  f34: number;
  f35: number;
  f36: number;
  f37: number;
  f38: number;
  f39: number;
  extra?: string;
}
export function main(): void {
  const wide: Wide = { f0: 0, f1: 1, f2: 2, f3: 3, f4: 4, f5: 5, f6: 6, f7: 7, f8: 8, f9: 9, f10: 10, f11: 11, f12: 12, f13: 13, f14: 14, f15: 15, f16: 16, f17: 17, f18: 18, f19: 19, f20: 20, f21: 21, f22: 22, f23: 23, f24: 24, f25: 25, f26: 26, f27: 27, f28: 28, f29: 29, f30: 30, f31: 31, f32: 32, f33: 33, f34: 34, f35: 35, f36: 36, f37: 37, f38: 38, f39: 39 };
  wide.f39 = 390;
  wide.extra = "x";
  console.log(String(Object.keys(wide).length) + " " + String(wide.f39));
  delete wide.extra;
  console.log(String(Object.hasOwn(wide, "extra")) + " " + String(JSON.stringify(wide).length));
}
