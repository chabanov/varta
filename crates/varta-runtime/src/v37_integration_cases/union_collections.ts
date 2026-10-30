// Multi-shape unions (language core) whose members hold Map, Set and
// Uint8Array storage (collections) and #private class state (class
// generics), discriminated by descriptor while collections run between
// awaits.
class Leaf {
  readonly kind = 'leaf';
  #bytes: Uint8Array;
  constructor(text: string) {
    this.#bytes = new TextEncoder().encode(text);
  }
  get size(): number {
    return this.#bytes.length;
  }
  first(): number {
    return this.#bytes[0] ?? -1;
  }
}
class Branch {
  readonly kind = 'branch';
  children = new Map<string, Leaf | Branch>();
  tags = new Set<string>();
  add(name: string, child: Leaf | Branch): Branch {
    this.children.set(name, child);
    this.tags.add(child.kind);
    return this;
  }
}
interface Summary {
  kind: 'summary';
  names: string[];
}
type Node = Leaf | Branch | Summary;

function size(node: Node): number {
  switch (node.kind) {
    case 'leaf':
      return node.size;
    case 'branch': {
      let total = 0;
      for (const [, child] of node.children) {
        total += size(child);
      }
      return total;
    }
    case 'summary':
      return node.names.length;
    default: {
      const unreachable: never = node;
      return unreachable;
    }
  }
}

function describe(node: Node): string {
  if (node instanceof Leaf) {
    return `leaf ${node.size} ${node.first()}`;
  }
  if ('children' in node) {
    return `branch ${[...node.children.keys()].join(',')} ${[...node.tags].join('+')}`;
  }
  return `summary ${node.names.join('/')}`;
}

export async function main(): Promise<void> {
  const root = new Branch().add('a', new Leaf('hé')).add('b', new Branch().add('c', new Leaf('xyz')));
  await Promise.resolve();
  const nodes: Node[] = [root, new Leaf('q'), { kind: 'summary', names: ['n', 'm'] }];
  for (const node of nodes) {
    await Promise.resolve();
    console.log(`${describe(node)} = ${size(node)}`);
  }
  const inner = root.children.get('b');
  await Promise.resolve();
  if (inner !== undefined && inner.kind === 'branch') {
    console.log(describe(inner));
  }
}
