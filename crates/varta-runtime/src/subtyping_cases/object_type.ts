interface Spec {
  method: string;
  form?: object;
  body?: unknown;
}
function encode(spec: Spec): string {
  if (spec.form) {
    const parts: string[] = [];
    for (const [key, value] of Object.entries(spec.form as Record<string, unknown>)) {
      if (value === undefined || value === null) continue;
      parts.push(`${key}=${typeof value === "object" ? JSON.stringify(value) : String(value)}`);
    }
    return parts.join("&");
  }
  return "none";
}
function size(o: object): number {
  return JSON.stringify(o).length;
}
export function main(): void {
  const form = JSON.parse('{"a":1,"b":"x","c":[1,2]}') as object;
  console.log(encode({ method: "POST", form }));
  console.log(encode({ method: "GET" }));
  const o: object = { z: true };
  console.log(`${size(o)} ${size([1, 2, 3])} ${typeof o} ${JSON.stringify(o)}`);
}
