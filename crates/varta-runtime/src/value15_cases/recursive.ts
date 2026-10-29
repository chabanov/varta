// Recursive types through arrays, record fields and index signatures.
export type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
interface Tree {
  name: string;
  children: Tree[];
}
interface Link {
  value: number;
  next: Link | null;
}
type Catalog = { [key: string]: Catalog | string };
function size(tree: Tree): number {
  let total = 1;
  for (const child of tree.children) {
    if (child !== undefined) {
      total = total + size(child);
    }
  }
  return total;
}
function depth(value: JsonValue): number {
  if (Array.isArray(value)) {
    let deepest = 0;
    for (const item of value) {
      if (item !== undefined) {
        const d = depth(item);
        deepest = d > deepest ? d : deepest;
      }
    }
    return deepest + 1;
  }
  if (typeof value === "object" && value !== null) {
    let deepest = 0;
    for (const key of Object.keys(value)) {
      const item = key === undefined ? undefined : value[key];
      if (item !== undefined) {
        const d = depth(item);
        deepest = d > deepest ? d : deepest;
      }
    }
    return deepest + 1;
  }
  return 0;
}
export function main(): void {
  const tree: Tree = { name: "root", children: [{ name: "a", children: [] }, { name: "b", children: [{ name: "c", children: [] }] }] };
  console.log(String(size(tree)) + " " + JSON.stringify(tree));
  let list: Link | null = null;
  for (let i = 0; i < 4; i++) {
    list = { value: i, next: list };
  }
  let sum = 0;
  let cursor = list;
  while (cursor !== null) {
    sum = sum + cursor.value;
    cursor = cursor.next;
  }
  console.log(String(sum) + " " + JSON.stringify(list));
  const doc: JsonValue = { name: "x", tags: ["a", 1, true, null], nested: { deep: [[{}]] } };
  console.log(String(depth(doc)) + " " + JSON.stringify(doc));
  const catalog: Catalog = { fruit: { apple: "red" }, plain: "text" };
  const fruit = catalog["fruit"];
  if (typeof fruit === "object") {
    fruit["pear"] = "green";
  }
  console.log(JSON.stringify(catalog));
}
