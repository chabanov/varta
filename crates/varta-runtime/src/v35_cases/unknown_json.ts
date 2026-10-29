// JSON.parse and checked parsing that produce unknown, and assertions from it.
interface ProblemDocument {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  instance?: string;
  [key: string]: unknown;
}
interface Item {
  id: number;
  tags: string[];
  meta: unknown;
}
type JsonValue = null | boolean | number | string | JsonValue[] | { [key: string]: JsonValue };
function show(value: unknown): string {
  if (typeof value === "string") {
    return "string:" + value;
  }
  if (typeof value === "number") {
    return "number:" + String(value);
  }
  if (value === null) {
    return "null";
  }
  if (Array.isArray(value)) {
    return "array";
  }
  return typeof value + ":" + JSON.stringify(value);
}
function attempt(label: string, text: string): string {
  try {
    return label + " " + JSON.stringify(JSON.parse(text));
  } catch (e) {
    return label + " " + String(e instanceof SyntaxError);
  }
}
export function main(): void {
  const text = '{"type":"urn:x","title":"Bad","status":400,"extra":{"a":[1,"b",null,true]},"0":7}';
  const value = JSON.parse(text);
  console.log(show(value) + " " + JSON.stringify(value));
  const parsed = JSON.parse('[1,"two",{"three":3},[false],null]');
  console.log(show(parsed) + " " + JSON.stringify(parsed));
  console.log(show(JSON.parse('"hi"')) + " " + show(JSON.parse("42.5")) + " " + show(JSON.parse("null")) + " " + show(JSON.parse("true")));
  const doc = JSON.parse(text) as ProblemDocument;
  console.log(String(doc.title) + " " + String(doc.status) + " " + JSON.stringify(Object.keys(doc)) + " " + show(doc["extra"]));
  const stored: unknown = JSON.parse('{"title":"t","n":1}');
  const asserted = stored as ProblemDocument;
  asserted.status = 201;
  console.log(String(asserted.title) + " " + JSON.stringify(asserted));
  const record = JSON.parse('{"a":1,"b":"x"}') as Record<string, unknown>;
  console.log(JSON.stringify(record) + " " + show(record["a"]) + " " + show(record["missing"]));
  const item = JSON.parse('{"id":1,"tags":["a","b"],"meta":{"k":[]}}') as Item;
  console.log(String(item.id) + " " + JSON.stringify(item.tags) + " " + JSON.stringify(item.meta));
  const json = JSON.parse('{"x":[1,{"y":null}]}') as JsonValue;
  console.log(JSON.stringify(json));
  const checked = varta.json.parse<unknown>('{"q":[true]}');
  const list = varta.json.parse<unknown[]>('[1,"a",{"b":null}]');
  console.log(JSON.stringify(checked) + " " + String(list.length) + " " + show(list[2]));
  const fromSchema = varta.json.parse<ProblemDocument>('{"status":500,"z":1}');
  console.log(String(fromSchema.status) + " " + JSON.stringify(fromSchema));
  console.log(attempt("syntax", "{bad") + " " + attempt("proto", '{"__proto__":1}') + " " + attempt("escaped", '{"\\u0041":"\\u00e9"}'));
}
