// Declared fields of a mixed record indexed by unknown that hold an array of
// optional records, a nested record and a dictionary (v36 checked declared
// fields): built by object literals, converted from JSON on the first read.
interface Problem {
  title?: string;
  errors?: Array<{ field?: string; message?: string }>;
  meta?: { id: string };
  labels?: Record<string, string>;
  [key: string]: unknown;
}
function describe(problem: Problem): string {
  let out = String(problem.title);
  for (const error of problem.errors ?? []) {
    if (error !== undefined) out += " " + String(error.field) + "=" + String(error.message);
  }
  const meta = problem.meta;
  const labels = problem.labels;
  return out + " " + (meta === undefined ? "-" : meta.id) + " " + (labels === undefined ? "-" : String(labels["k"]));
}
export function main(): void {
  const built: Problem = { title: "built", errors: [{ field: "a" }], meta: { id: "m1" } };
  const list = built.errors;
  if (list !== undefined) list.push({ message: "pushed" });
  console.log(describe(built));
  const text = '{"title":"parsed","errors":[{"message":"x","field":"b"}],"labels":{"k":"v"},"extra":1}';
  const plain = JSON.parse(text) as Record<string, unknown> as Problem;
  console.log(describe(plain) + " " + String(plain.errors === plain.errors));
  console.log(String(JSON.stringify(plain)));
  console.log(describe(JSON.parse(text) as Problem));
}
