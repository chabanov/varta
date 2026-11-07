// V39 internal JSON-graph snapshot preparation. The work list and ancestor
// path stay on the managed heap; no recursive Wasm call grows with input depth.
interface CopyTask {
  value: unknown;
  parent: unknown;
  key: string;
  index: number;
  exit: boolean;
}

export function snapshotForgetKey(order: string[], key: string): void {
  let write = 0;
  for (let read = 0; read < order.length; read++) {
    const name = order[read];
    if (name !== key) { order[write] = name; write++; }
  }
  order.length = write;
}

export function snapshotSetKey(order: string[], key: string, present: boolean): void {
  if (!present) {
    snapshotForgetKey(order, key);
    order.push(key);
  }
}

export function snapshotClone(value: unknown, depthLimit: number, allowUndefined: boolean): unknown {
  const result: Record<string, unknown> = {};
  const pending: CopyTask[] = [{ value, parent: result, key: "value", index: -1, exit: false }];
  const path: unknown[] = [];
  while (pending.length !== 0) {
    const task = pending.pop();
    if (task === undefined) throw new TypeError('invalid snapshot work list');
    if (task.exit) { path.pop(); continue; }
    const original = task.value;
    const current = original;
    let copied: unknown = current;
    if (current !== null && typeof current === 'object') {
      if (path.length >= depthLimit) throw new TypeError('snapshot exceeds the JSON depth limit');
      pending.push({ value: null, parent: null, key: "", index: -1, exit: true });
      if (Array.isArray(current)) {
        const entries = snapshotArray(original);
        for (const ancestor of path) {
          if (snapshotSame(ancestor, original)) throw new TypeError('snapshot cannot copy a cyclic graph');
        }
        path.push(original);
        const array: unknown[] = [];
        copied = array;
        for (let i = entries.length - 1; i >= 0; i--) {
          if (!Object.hasOwn(entries, i)) throw new TypeError('snapshot requires dense JSON arrays');
          pending.push({ value: entries[i], parent: array, key: "", index: i, exit: false });
        }
      } else {
        const object = snapshotObject(current);
        for (const ancestor of path) {
          if (snapshotSame(ancestor, original)) throw new TypeError('snapshot cannot copy a cyclic graph');
        }
        path.push(original);
        const clone: Record<string, unknown> = {};
        copied = clone;
        const keys = Object.keys(object);
        for (let i = keys.length - 1; i >= 0; i--) {
          const key = keys[i];
          pending.push({ value: object[key], parent: clone, key, index: -1, exit: false });
        }
      }
    } else if (typeof current === 'number') {
      if (!Number.isFinite(current)) throw new TypeError('snapshot requires finite JSON numbers');
    } else if (current !== null && typeof current !== 'string' && typeof current !== 'boolean'
      && !(allowUndefined && current === undefined)) {
      throw new TypeError('snapshot requires JSON-shaped data');
    }
    const parent = task.parent;
    if (Array.isArray(parent)) {
      parent[task.index] = copied;
    } else {
      const object = parent as Record<string, unknown>;
      snapshotPut(object, task.key, copied);
    }
  }
  return result["value"];
}

// Replaced by the compiler only inside this engine prelude. It refuses class
// instances and accessor objects before reading an application property.
export function snapshotObject(value: unknown): Record<string, unknown> {
  return value as Record<string, unknown>;
}

export function snapshotSame(a: unknown, b: unknown): boolean { return false; }

export function snapshotArray(value: unknown): unknown[] { return []; }

// JSON copies define an own data key; ordinary source assignment still
// refuses the prototype setter, including a dynamic __proto__ key.
export function snapshotPut(target: Record<string, unknown>, key: string, value: unknown): void {
  target[key] = value;
}
