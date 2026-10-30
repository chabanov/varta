// A line protocol reader: a deferred binding assigned in a loop test, a
// string switch with a scoped clause, dense string reads and a closure that
// resets captured mutable state.
interface Frame { kind: string; body: string; seq?: number }
class Done {
  seen = false;
  mark(): void {
    this.seen = true;
  }
}
function quoted(text: string, key: string): string | undefined {
  const at = text.indexOf(key + "=");
  if (at < 0) return undefined;
  let i = at + key.length + 1;
  if (i >= text.length || text[i] !== "'") return undefined;
  i++;
  const begin = i;
  while (i < text.length) {
    const unit = text[i];
    if (unit === "\\") {
      i += 2;
      continue;
    }
    if (unit === "'") break;
    i++;
  }
  return i > begin ? text.slice(begin, i) : undefined;
}
function concat(parts: string[]): string {
  let out = "";
  for (const part of parts) out += out.length > 0 ? "+" + part : part;
  return out;
}
function read(input: string, done?: Done): Frame[] {
  const frames: Frame[] = [];
  let rest = input;
  let parts: string[] = [];
  let kind = "";
  let seq: number | undefined;
  let pending = false;
  const emit = (): Frame | undefined => {
    if (!pending) return undefined;
    pending = false;
    const body = concat(parts);
    const frameKind = kind || quoted(body, "kind") || "plain";
    const frameSeq = seq;
    parts = [];
    kind = "";
    seq = undefined;
    return body.length === 0 ? undefined : { kind: frameKind, body, seq: frameSeq };
  };
  let end: number;
  while ((end = rest.indexOf(";")) !== -1) {
    const line = rest.slice(0, end);
    rest = rest.slice(end + 1);
    if (line === "") {
      const frame = emit();
      if (frame) frames.push(frame);
      continue;
    }
    const colon = line.indexOf(":");
    const name = colon === -1 ? line : line.slice(0, colon);
    const value = colon === -1 ? "" : line.slice(colon + 1);
    pending = true;
    switch (name) {
      case "kind":
        kind = value;
        break;
      case "part":
        if (value === "END") {
          done?.mark();
          const last = emit();
          if (last) frames.push(last);
          return frames;
        }
        parts.push(value);
        break;
      case "seq": {
        const parsed = Number.parseInt(value, 10);
        if (Number.isFinite(parsed)) seq = parsed;
        break;
      }
      default:
        break;
    }
  }
  const tail = emit();
  if (tail) frames.push(tail);
  return frames;
}
export function main(): void {
  const done = new Done();
  const frames = read("kind:a;part:x;part:kind='q';seq:4;;part:y;;seq:z;part:w;part:END;", done);
  for (const frame of frames) console.log(frame.kind + " " + frame.body + " " + String(frame.seq));
  console.log(String(done.seen) + " " + String(quoted("k='a\\'b'", "k")) + " " + String(quoted("x", "k")));
  console.log(String(read("part:open;").length));
}
