class Tree implements Iterable<string> {
  constructor(public label: string, public children: Tree[]) {}
  *[Symbol.iterator](): Generator<string> {
    yield this.label;
    for (const child of this.children) yield* child;
  }
}
function* numbered(tree: Tree): Generator<string, number> {
  let n = 0;
  for (const label of tree) yield `${++n}. ${label}`;
  return n;
}
function* report(tree: Tree): Generator<string> {
  const count = yield* numbered(tree);
  yield `${count} nodes`;
}
export function main(): void {
  const tree = new Tree("root", [new Tree("a", [new Tree("a1", [])]), new Tree("b", [])]);
  console.log([...tree].join(" ")); // root a a1 b
  const extra: string[] = ["z"];
  console.log([...tree, ...extra, ...new Set(["s"])].length); // 6
  console.log(Array.from(report(tree)).join(", ")); // 1. root, 2. a, 3. a1, 4. b, 4 nodes
}
