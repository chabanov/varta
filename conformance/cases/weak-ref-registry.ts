// WeakRef and FinalizationRegistry (ECMA-262 §26.1, §26.2) without a
// collection: a live target is dereferenced, unregister reports whether it
// removed cells, and a held value that is its target is a TypeError.
class Target {
  id: number;
  constructor(id: number) {
    this.id = id;
  }
}

export function main(): void {
  const target = new Target(7);
  const ref = new WeakRef(target);
  console.log(`${ref.deref()?.id} ${ref.deref() === target}`);
  const registry = new FinalizationRegistry<string>((held) => {
    console.log(`cleanup ${held}`);
  });
  const token = new Target(0);
  registry.register(target, 'target', token);
  registry.register(new Target(1), 'other', token);
  console.log(`${registry.unregister(token)} ${registry.unregister(token)}`);
  const objects = new FinalizationRegistry<Target>((held) => {
    console.log(`cleanup ${held.id}`);
  });
  try {
    objects.register(target, target);
  } catch (error) {
    console.log(error instanceof TypeError ? error.message : 'no TypeError');
  }
}
