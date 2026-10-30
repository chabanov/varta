// A generic fine-grained reactive keyed list, shaped like a UI library's
// signal/effect/mapKeyed layer: every row owns a text signal and a render
// effect that rebuilds its node; "update" reverses the rows and changes the
// text of every row. `codegen_v2_tests` replaces the two constants below.
const ROWS = 10;
const UPDATE = false;

interface Node { tag: string; text: string; attrs: string[]; children: Node[] }
interface Owner { stack: (() => void)[]; runs: number }
interface Signal { get: () => string; set: (next: string) => void }
interface Row { key: string; label: Signal; node: Node }
interface Item { key: string; text: string }

function signal(owner: Owner, initial: string): Signal {
  let value = initial;
  const subscribers: (() => void)[] = [];
  return {
    get: (): string => {
      const depth = owner.stack.length;
      if (depth > 0) {
        const current = owner.stack[depth - 1];
        if (subscribers.indexOf(current) < 0) { subscribers.push(current); }
      }
      return value;
    },
    set: (next: string): void => {
      if (next === value) { return; }
      value = next;
      for (const run of subscribers) { run(); }
    },
  };
}

function effect(owner: Owner, run: () => void): void {
  owner.stack.push(run);
  owner.runs++;
  run();
  owner.stack.pop();
}

function mapKeyed(owner: Owner, items: Item[], rows: Row[], parent: Node): Row[] {
  const index: Record<string, number> = {};
  for (let i = 0; i < rows.length; i++) { index[rows[i].key] = i; }
  const next: Row[] = [];
  const children: Node[] = [];
  for (const item of items) {
    const at = index[item.key];
    if (at === undefined) {
      const label = signal(owner, item.text);
      const row: Row = { key: item.key, label, node: { tag: "li", text: "", attrs: [], children: [] } };
      effect(owner, (): void => {
        const text = label.get();
        const span: Node = { tag: "span", text, attrs: ["class", "label"], children: [] };
        row.node = { tag: "li", text, attrs: ["data-key", row.key], children: [span] };
        owner.runs++;
      });
      next.push(row);
      children.push(row.node);
    } else {
      const row = rows[at];
      row.label.set(item.text);
      next.push(row);
      children.push(row.node);
    }
  }
  parent.children = children;
  return next;
}

function list(count: number, prefix: string, reversed: boolean): Item[] {
  const out: Item[] = [];
  for (let i = 0; i < count; i++) {
    const k = reversed ? count - 1 - i : i;
    out.push({ key: "k" + String(k), text: prefix + String(k) });
  }
  return out;
}

export function main(): void {
  const owner: Owner = { stack: [], runs: 0 };
  const parent: Node = { tag: "ul", text: "", attrs: [], children: [] };
  let rows = mapKeyed(owner, list(ROWS, "row ", false), [], parent);
  if (UPDATE) {
    rows = mapKeyed(owner, list(ROWS, "changed ", true), rows, parent);
  }
  let size = 0;
  for (const child of parent.children) { size += child.text.length + child.children.length; }
  console.log(String(rows.length) + " " + String(owner.runs) + " " + String(size));
}
