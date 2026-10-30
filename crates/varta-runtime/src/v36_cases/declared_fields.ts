// Checked declared fields: mixed records whose declared fields hold arrays of
// records, nested records, dictionaries, unions and nested mixed records.
interface FieldError {
  field?: string;
  message?: string;
}
interface ProblemDocument {
  type?: string;
  title?: string;
  status?: number;
  detail?: string | FieldError[];
  errors?: Array<{ field?: string; message?: string }>;
  meta?: { requestId: string; tags?: string[] };
  labels?: Record<string, string>;
  extensions?: { code?: string; [key: string]: unknown };
  [key: string]: unknown;
}
function count(doc: ProblemDocument): number {
  const errors = doc.errors;
  return errors === undefined ? -1 : errors.length;
}
function fields(doc: ProblemDocument): string {
  const errors = doc.errors ?? [];
  let out = "";
  for (const e of errors) {
    if (e !== undefined) out += String(e.field) + "=" + String(e.message) + ";";
  }
  return out;
}
export function main(): void {
  // Object literals build the declared types; reads return the stored values.
  const built: ProblemDocument = {
    title: "Bad",
    errors: [{ field: "a", message: "m" }],
    meta: { requestId: "r1", tags: ["x"] },
    labels: { env: "prod" },
    extensions: { code: "E1", hint: 3 },
    trace: [1, 2],
  };
  console.log(JSON.stringify(built));
  const first = built.errors;
  console.log(String(first === built.errors) + " " + String(built.meta === built.meta));
  if (first !== undefined) first.push({ message: "second" });
  console.log(String(count(built)) + " " + fields(built));
  const meta = built.meta;
  if (meta !== undefined) console.log(meta.requestId + " " + JSON.stringify(meta.tags));
  const labels = built.labels;
  if (labels !== undefined) console.log(String(labels["env"]) + " " + String(Object.keys(labels).length));
  const extensions = built.extensions;
  if (extensions !== undefined) console.log(String(extensions.code) + " " + JSON.stringify(extensions));

  // Writes are typed by the declaration; delete removes an optional field.
  built.errors = [{ field: "b" }];
  built.detail = [{ message: "as list" }];
  console.log(fields(built) + " " + JSON.stringify(built.detail));
  delete built.errors;
  console.log(String(count(built)) + " " + String("errors" in built));
  built.detail = "plain";
  console.log(JSON.stringify(built.detail));

  // JSON values convert on the first declared read and are stored back, so
  // later reads see one value.
  const text =
    '{"type":"about:blank","status":422,"errors":[{"message":"required","field":"name"},{"field":"age"}],' +
    '"meta":{"tags":["a","b"],"requestId":"r2"},"labels":{"x":"1"},"extensions":{"code":"E2","n":[1]},"trace":"t"}';
  const parsed: unknown = JSON.parse(text);
  if (parsed !== null && typeof parsed === "object" && !Array.isArray(parsed)) {
    const body = parsed as Record<string, unknown>;
    const doc = body as ProblemDocument;
    console.log(fields(doc) + " " + String(doc.errors === doc.errors));
    const errors = doc.errors;
    if (errors !== undefined) errors.push({ field: "added" });
    console.log(fields(doc));
    const docMeta = doc.meta;
    if (docMeta !== undefined) console.log(docMeta.requestId + " " + JSON.stringify(docMeta.tags));
    const docLabels = doc.labels;
    if (docLabels !== undefined) console.log(String(docLabels["x"]));
    const docExtensions = doc.extensions;
    if (docExtensions !== undefined) console.log(String(docExtensions.code));
    console.log(JSON.stringify(doc));
  }

  // Checked parsing proves the declared fields of the root.
  const direct = JSON.parse(text) as ProblemDocument;
  console.log(String(direct.status) + " " + fields(direct) + " " + JSON.stringify(direct.meta));
  const checked = varta.json.parse<ProblemDocument>(text);
  console.log(String(checked.type) + " " + String(count(checked)));
  console.log(String(JSON.stringify(checked) === JSON.stringify(direct)));

  // An empty array read through a declared field is stored, so pushes stay.
  const empty: ProblemDocument = { errors: [] };
  const list = empty.errors;
  if (list !== undefined) list.push({ field: "late" });
  console.log(JSON.stringify(empty));

  // Spreads copy every entry; declared fields keep their types.
  const copy: ProblemDocument = { ...direct, title: "copied" };
  console.log(String(copy.title) + " " + fields(copy) + " " + String(copy.status));

  // Assertions from unknown convert JSON arrays and objects.
  const raw: unknown = JSON.parse('[{"field":"f"},{"message":"g"}]');
  const asserted = raw as FieldError[];
  const head = asserted[0];
  console.log(String(asserted.length) + " " + (head === undefined ? "-" : String(head.field)));
  const record: unknown = JSON.parse('{"requestId":"r3"}');
  const proven = record as { requestId: string; tags?: string[] };
  console.log(proven.requestId + " " + String(proven.tags === undefined));
}
