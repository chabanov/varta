function describe(value: unknown): string {
  if (Array.isArray(value)) {
    let parts = "";
    for (const item of value) {
      if (item !== undefined && item !== null) parts += `${typeof item}:${String(item)},`;
    }
    return `[${value.length}] ${parts}`;
  }
  return typeof value;
}
function query(params: Record<string, unknown>): string {
  const pairs: string[] = [];
  for (const [key, value] of Object.entries(params)) {
    if (value === undefined || value === null) continue;
    if (Array.isArray(value)) {
      for (const item of value) {
        if (item !== undefined && item !== null) pairs.push(`${key}=${String(item)}`);
      }
    } else {
      pairs.push(`${key}=${String(value)}`);
    }
  }
  return pairs.join("&");
}
export function main(): void {
  console.log(describe([1, 2, 3]));
  console.log(describe(["a", "b"]));
  console.log(describe(JSON.parse('[1,"x",true,null,{"a":1}]')));
  console.log(describe([true, false]));
  console.log(JSON.stringify([[1], [2, 3]].length));
  console.log(describe("text"));
  console.log(query({ tag: ["a", "b"], n: 3, skip: undefined, ids: [1, 2] }));
  const parsed: unknown = JSON.parse('{"items":[{"id":1},{"id":2}]}');
  const page = parsed as Record<string, unknown>;
  const items = page["items"];
  if (Array.isArray(items)) {
    console.log(`${items.length} ${JSON.stringify(items[1])}`);
  }
}
