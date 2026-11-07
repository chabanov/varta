// Map and Set keyed by a class whose arrays are shared with arrays of a
// union admitting null (K9 A step 4): the key vector holds union cells and
// the hash probes compare the instances they hold.
class A { n: number = 1; }
class B extends A { tag: string = "b"; }
function nullable(list: (A | null)[]): number { list.push(null); return list.length; }
export function main(): void {
  const as: A[] = [new A(), new B()];
  console.log(nullable(as));
  const counts = new Map<A, number>();
  const a = new A();
  counts.set(a, 1);
  counts.set(new B(), 2);
  counts.set(a, 3);
  console.log(`${counts.size} ${counts.get(a)} ${counts.has(new A())}`);
  const seen = new Set<A>([a, a, new B()]);
  console.log(`${seen.size} ${seen.has(a)}`);
  for (const [key, value] of counts) {
    console.log(`${key.n} ${value}`);
  }
  counts.delete(a);
  console.log(`${counts.size} ${[...seen].length}`);
}
