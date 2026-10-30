// Temporal dead zones (language core) with #private state, generic classes
// and generic methods (class generics): a callback stored in a private
// field captures a binding declared after it, and a call before that
// declaration throws a ReferenceError.
class Registry<T> {
  #items: T[] = [];
  #onAdd: (item: T) => void;
  constructor(onAdd: (item: T) => void) {
    this.#onAdd = onAdd;
  }
  add(item: T): number {
    this.#items.push(item);
    this.#onAdd(item);
    return this.#items.length;
  }
  map<U>(f: (item: T) => U): U[] {
    const out: U[] = [];
    for (const item of this.#items) {
      out.push(f(item));
    }
    return out;
  }
  owns(value: Registry<T> | Plain): boolean {
    return #items in value;
  }
}
class Plain {
  items: number[] = [];
}

function build(): string {
  const registry = new Registry<number>((n: number): void => {
    events.push(`+${n}`);
  });
  const events: string[] = [];
  registry.add(1);
  registry.add(2);
  const labels = registry.map<string>((n: number): string => `#${n}`);
  return `${labels.join(',')} ${events.join('')} ${registry.owns(registry)} ${registry.owns(new Plain())}`;
}

function early(): string {
  const registry = new Registry<string>((s: string): void => {
    seen.push(s);
  });
  try {
    registry.add('x');
  } catch (error) {
    return error instanceof Error ? `${error.name} ${registry.map<number>((s: string): number => s.length).length}` : 'other';
  }
  const seen: string[] = [];
  return `no ${seen.length}`;
}

export function main(): void {
  console.log(build());
  console.log(early());
}
