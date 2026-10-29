// Conversions of JSON values to recursive, nested and union declared types,
// and the v36 completions around them: destructuring dictionaries, property
// path typeof narrowing, `?? []`, `let x: unknown;` and exhaustive switches.
interface Item {
  id: string;
  tags?: string[];
}
type Tree = { name: string; kids?: Tree[] };
interface Doc {
  items?: Item[];
  tree?: Tree;
  matrix?: number[][];
  values?: unknown[];
  choice?: Item | Record<string, number>;
  either?: string | Item[];
  [key: string]: unknown;
}
function depth(tree: Tree): number {
  let deepest = 0;
  for (const kid of tree.kids ?? []) {
    if (kid !== undefined) deepest = Math.max(deepest, depth(kid));
  }
  return deepest + 1;
}
function kind(value: unknown): string {
  const code = typeof value === "string" ? 1 : typeof value === "number" ? 2 : 0;
  switch (code) {
    case 1:
      return "text";
    case 2:
      return "number";
    default:
      if (value === null) return "null";
      return "other";
  }
}
function message(body: Record<string, unknown>): string {
  const { error, message: text } = body;
  if (typeof error === "string") return error;
  return typeof body.message === "string" ? body.message : String(text);
}
export function main(): void {
  const doc = JSON.parse(
    '{"items":[{"id":"a","tags":["x"]},{"id":"b"}],' +
      '"tree":{"name":"root","kids":[{"name":"leaf"},{"name":"mid","kids":[{"name":"deep"}]}]},' +
      '"matrix":[[1,2],[3]],"values":[1,"two",null],"choice":{"a":1,"b":2}}',
  ) as Record<string, unknown> as Doc;
  console.log(String(JSON.stringify(doc.items)) + " " + String(JSON.stringify(doc.matrix)));
  const tree = doc.tree;
  if (tree !== undefined) console.log(tree.name + " " + String(depth(tree)));
  const values = doc.values ?? [];
  let kinds = "";
  for (const value of values) kinds += kind(value) + ",";
  console.log(kinds);
  const choice = doc.choice;
  console.log(String(JSON.stringify(choice)) + " " + String(choice === doc.choice));
  const items = doc.items;
  if (items !== undefined) {
    items.push({ id: "c", tags: [] });
    console.log(String(doc.items === items) + " " + String(JSON.stringify(doc)));
  }
  let later: unknown;
  console.log(String(later === undefined));
  later = JSON.parse('{"message":"from message"}');
  if (later !== null && typeof later === "object") {
    const body = later as Record<string, unknown>;
    console.log(message(body) + " " + message({ error: "from error" }));
  }
  const listed = JSON.parse('{"either":[{"id":"u"}]}') as Record<string, unknown> as Doc;
  const texted = JSON.parse('{"either":"plain"}') as Record<string, unknown> as Doc;
  console.log(String(JSON.stringify(listed.either)) + " " + String(JSON.stringify(texted.either)));
  const tagged: Record<string, unknown> = { reason: "r", count: 3 };
  const reason = typeof tagged.reason === "string" ? tagged.reason : "none";
  const count = typeof tagged.count !== "number" ? -1 : tagged.count;
  console.log(reason + " " + String(count + 1));
}
