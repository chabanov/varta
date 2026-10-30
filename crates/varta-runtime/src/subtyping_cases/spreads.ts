interface Event {
  id?: string;
  event: string;
  data: string;
  retry?: number;
  size(): number;
}
interface Spec {
  method: string;
  path: string;
  headers?: Record<string, unknown>;
  query?: Record<string, unknown>;
}
function make(fields: { id?: string; event: string; data: string; retry?: number }): Event {
  return {
    ...fields,
    size(): number {
      return fields.data.length;
    },
  };
}
function withPath(spec: Spec, path: string): Spec {
  return { ...spec, path };
}
interface StreamOptions {
  reconnect?: boolean;
  retries?: number;
  label?: string;
}
function finalOptions(stream: StreamOptions | undefined, label: string): StreamOptions {
  return { ...stream, label: stream?.label ?? label, reconnect: false };
}
export function main(): void {
  console.log(JSON.stringify(finalOptions(undefined, "x")));
  const final = finalOptions({ retries: 3, reconnect: true }, "y");
  console.log(`${final.retries} ${final.reconnect} ${final.label}`);
  const a = make({ event: "e", data: "abc" });
  const b = make({ id: "7", event: "f", data: "xy", retry: 5 });
  console.log(`${a.id} ${a.event} ${a.size()} ${"id" in a} ${a.retry}`);
  console.log(`${b.id} ${b.event} ${b.size()} ${"id" in b} ${b.retry}`);
  const base = { x: 1, y: "two" };
  const over = { ...base, y: "three", z: true };
  console.log(JSON.stringify(over));
  const under = { y: "zero", ...base };
  console.log(JSON.stringify(under));
  const s = withPath({ method: "GET", path: "/a", query: { q: 1 } }, "/b");
  console.log(`${s.method} ${s.path} ${JSON.stringify(s.query)} ${s.headers === undefined}`);
  const spec: Spec = { method: "POST", path: "/p", query: { a: 1 } };
  const extra = { b: 2 };
  const query = { ...spec.query, ...extra };
  console.log(JSON.stringify(query));
  const headers: Record<string, unknown> = { ...spec.headers, Accept: "text/event-stream" };
  console.log(JSON.stringify(headers));
}
