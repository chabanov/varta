// Mixed index records: declared fields next to a string index signature.
interface ProblemDocument {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  instance?: string;
  [key: string]: unknown;
}
interface Named {
  id: string;
  label?: string;
  [key: string]: string | undefined;
}
interface Base {
  kind: string;
  [key: string]: string | number;
}
interface Derived extends Base {
  size: number;
}
type Inline = { name: string; [key: string]: string };
function describe(doc: ProblemDocument): string {
  const title = doc.title;
  const status = doc.status;
  return (title === undefined ? "?" : title) + "/" + (status === undefined ? "-" : String(status));
}
function plain(values: Record<string, unknown>): string {
  return JSON.stringify(Object.keys(values));
}
export function main(): void {
  const doc: ProblemDocument = { type: "about:blank", status: 404, extra: true, "2": "two" };
  doc.title = "Not Found";
  doc["trace"] = [1, 2];
  console.log(JSON.stringify(doc));
  console.log(describe(doc));
  console.log(JSON.stringify(Object.keys(doc)) + " " + plain(doc));
  console.log(String("extra" in doc) + " " + String("detail" in doc) + " " + String(Object.hasOwn(doc, "status")));
  console.log(typeof doc["extra"]);
  delete doc.status;
  delete doc["extra"];
  console.log(JSON.stringify(doc) + " " + describe(doc));
  const key: string = "title";
  console.log(String(doc[key]));
  const named: Named = { label: "L", id: "x1", other: "o" };
  named.id = "x2";
  named["dyn"] = "d";
  console.log(named.id + " " + String(named.label) + " " + JSON.stringify(named));
  const derived: Derived = { size: 3, kind: "box", color: "red" };
  derived.size = derived.size + 1;
  console.log(JSON.stringify(derived) + " " + derived.kind + " " + String(derived["color"]));
  const inline: Inline = { name: "n", "1": "one" };
  console.log(JSON.stringify(inline) + " " + inline.name + " " + JSON.stringify(Object.entries(inline)));
}
