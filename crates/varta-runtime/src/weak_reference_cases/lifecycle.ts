// The weak reference lifecycle (ECMA-262 §9.9, §9.10, §26.1, §26.2) with
// explicit collections (`gc()`, Node's --expose-gc): weak keys and members
// do not keep their targets alive, a WeakMap value lives exactly as long as
// its key (ephemerons, also through chains and cycles), a WeakRef's target
// stays alive until the end of the turn that created or dereferenced it
// (KeepDuringJob), and a FinalizationRegistry calls its callback in a job of
// its own after the collection, the most recent registration first, never
// for an unregistered cell.
class Node {
  label: string;
  next: Node | undefined;
  constructor(label: string) {
    this.label = label;
    this.next = undefined;
  }
}

class Holder {
  value: Node | undefined;
  constructor() {
    this.value = undefined;
  }
}

function turn(): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
}

function alive(ref: WeakRef<Node>): string {
  const target = ref.deref();
  return target === undefined ? 'collected' : target.label;
}

// Fresh objects reachable only through what the helpers return or store.
function keyed(map: WeakMap<Node, Node>, refs: WeakRef<Node>[]): void {
  const key = new Node('key');
  const value = new Node('value');
  // The value refers back to its key: still no strong path to either.
  value.next = key;
  map.set(key, value);
  refs.push(new WeakRef(key), new WeakRef(value));
}

function chained(map: WeakMap<Node, Node>, holder: Holder, refs: WeakRef<Node>[]): void {
  const first = new Node('first');
  const second = new Node('second');
  const third = new Node('third');
  map.set(first, second);
  map.set(second, third);
  holder.value = first;
  refs.push(new WeakRef(first), new WeakRef(second), new WeakRef(third));
}

function registered(
  registry: FinalizationRegistry<string>,
  token: Node,
  refs: WeakRef<Node>[],
): void {
  for (let i = 0; i < 4; i++) {
    const target = new Node(`target ${i}`);
    registry.register(target, `held ${i}`, i === 1 ? token : undefined);
    refs.push(new WeakRef(target));
  }
}

// The label at the end of the chain from the held key (no binding of the
// caller keeps a node).
function lookup(map: WeakMap<Node, Node>, holder: Holder): string {
  const first = holder.value;
  if (first === undefined) {
    return 'no first';
  }
  const second = map.get(first);
  if (second === undefined) {
    return 'no second';
  }
  return String(map.get(second)?.label);
}

export async function main(): Promise<void> {
  const map = new WeakMap<Node, Node>();
  const refs: WeakRef<Node>[] = [];
  keyed(map, refs);
  gc();
  // Created in this turn: kept until it ends.
  console.log(`${alive(refs[0])} ${alive(refs[1])}`);
  await turn();
  gc();
  console.log(`${alive(refs[0])} ${alive(refs[1])}`);

  const holder = new Holder();
  const chain: WeakRef<Node>[] = [];
  chained(map, holder, chain);
  await turn();
  gc();
  console.log(`${alive(chain[0])} ${alive(chain[1])} ${alive(chain[2])}`);
  console.log(lookup(map, holder));
  holder.value = undefined;
  await turn();
  gc();
  console.log(`${alive(chain[0])} ${alive(chain[1])} ${alive(chain[2])}`);

  const set = new WeakSet<Node>();
  const member = new Node('member');
  set.add(member);
  const memberRef = new WeakRef(member);
  await turn();
  gc();
  console.log(`${alive(memberRef)} ${set.has(member)}`);

  const cleaned: string[] = [];
  const registry = new FinalizationRegistry<string>((held) => {
    cleaned.push(held);
    console.log(`cleanup ${held}`);
  });
  const token = new Node('token');
  const targets: WeakRef<Node>[] = [];
  registered(registry, token, targets);
  console.log(`unregister ${registry.unregister(token)} ${registry.unregister(token)}`);
  await turn();
  gc();
  console.log(`after gc ${cleaned.length}`);
  await turn();
  console.log(`after turn ${cleaned.join(', ')}`);
  console.log(`${alive(targets[0])} ${alive(targets[1])} ${alive(targets[3])}`);

  // A held target is never collected; a WeakRef read in this turn keeps it.
  const kept = new Node('kept');
  registry.register(kept, 'kept held');
  const keptRef = new WeakRef(kept);
  await turn();
  gc();
  console.log(`${alive(keptRef)} ${kept.label}`);
  await turn();
  console.log(`cleaned ${cleaned.length}`);
}
