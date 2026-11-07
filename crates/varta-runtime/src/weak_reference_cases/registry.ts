// FinalizationRegistry and WeakRef edges (ECMA-262 §26.1, §26.2):
// CanBeHeldWeakly for targets and tokens (objects and unregistered
// symbols), a target that is its own token, held values of every kind, a
// token that is collected before its target, unregistering a cell whose
// target was already collected, a callback that registers again, and a
// registry that is itself collected (no callbacks).
class Item {
  id: number;
  constructor(id: number) {
    this.id = id;
  }
}

function describe(error: unknown): string {
  return error instanceof Error ? `${error.name}: ${error.message}` : 'not an error';
}

function turn(): Promise<void> {
  return new Promise<void>((resolve) => {
    setTimeout(resolve, 0);
  });
}

const events: string[] = [];

function selfTokens(registry: FinalizationRegistry<string>): void {
  for (let i = 0; i < 3; i++) {
    const item = new Item(i);
    registry.register(item, `self ${i}`, item);
  }
}

function symbolTargets(registry: FinalizationRegistry<string>): void {
  registry.register(Symbol('a'), 'symbol a');
  registry.register(Symbol(), 'symbol b');
}

function collectedToken(registry: FinalizationRegistry<string>, keep: Item[]): void {
  const target = new Item(10);
  keep.push(target);
  registry.register(target, 'token died', new Item(11));
}

function lost(): void {
  const registry = new FinalizationRegistry<string>((held) => {
    events.push(`lost ${held}`);
  });
  registry.register(new Item(20), 'never');
}

function pending(registry: FinalizationRegistry<number>, token: Item): void {
  registry.register(new Item(30), 30, token);
  registry.register(new Item(31), 31);
}

export async function main(): Promise<void> {
  const registry = new FinalizationRegistry<string>((held) => {
    events.push(held);
  });
  const item = new Item(1);
  try {
    registry.register(Symbol.for('registered'), 'x');
  } catch (error) {
    console.log(describe(error));
  }
  try {
    registry.register(item, 'x', Symbol.for('token'));
  } catch (error) {
    console.log(describe(error));
  }
  try {
    registry.unregister(Symbol.for('token'));
  } catch (error) {
    console.log(describe(error));
  }
  const numbers = new FinalizationRegistry<Item>((held) => {
    events.push(`item ${held.id}`);
  });
  try {
    numbers.register(item, item);
  } catch (error) {
    console.log(describe(error));
  }
  try {
    new WeakRef(Symbol.for('registered'));
  } catch (error) {
    console.log(describe(error));
  }
  const symbol = Symbol('weak');
  const ref = new WeakRef(symbol);
  console.log(String(ref.deref() === symbol));
  console.log(`${registry.unregister(item)} ${registry.unregister(symbol)}`);

  selfTokens(registry);
  const keep: Item[] = [];
  collectedToken(registry, keep);
  lost();
  await turn();
  gc();
  await turn();
  // The order of one cleanup is unspecified (§26.2.3.3 chooses any cell),
  // and V8's depends on its heap: sorted.
  console.log(events.sort().join(', '));
  events.length = 0;
  // V8 cleans symbol targets apart from object targets: one kind per turn.
  symbolTargets(registry);
  await turn();
  gc();
  await turn();
  console.log(events.join(', '));
  events.length = 0;
  // The token died with its own cell; the other token was collected.
  console.log(`${registry.unregister(keep[0])}`);

  const counts: FinalizationRegistry<number> = new FinalizationRegistry<number>((held) => {
    events.push(`count ${held}`);
    if (held === 31) {
      counts.register(new Item(32), 32);
    }
  });
  const token = new Item(3);
  pending(counts, token);
  await turn();
  gc();
  // Collected but not cleaned up yet: unregister still removes the cell.
  console.log(`${counts.unregister(token)} ${counts.unregister(token)}`);
  await turn();
  console.log(events.join(', '));
  events.length = 0;
  gc();
  await turn();
  console.log(events.join(', '));
  // The registries stay in use until here: a registry that nothing
  // reaches any more may be collected with its cells (Varta's frames keep
  // only live bindings, V8's keep every captured one).
  console.log(`${keep.length} ${item.id} ${registry.unregister(item)} ${counts.unregister(token)}`);
}
