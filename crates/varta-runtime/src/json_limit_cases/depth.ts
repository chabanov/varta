// V38 `data_limits`: values nested past the depth of 64 of earlier profiles
// round-trip through checked JSON; values built in the guest serialize too.
interface Node {
  c: Node[];
}
type Tree = { child: Tree | null; n: number };

export function main(): void {
  // Each level is an object and an array: JSON depth 2 * levels - 1.
  for (const levels of [50, 500, 3000]) {
    const text = '{"c":['.repeat(levels) + "]}".repeat(levels);
    const value = varta.json.parse<Node>(text);
    console.log(`${levels} ${JSON.stringify(value) === text}`);
  }
  let tree: Tree = { child: null, n: 0 };
  for (let n = 1; n < 3000; n++) {
    tree = { child: tree, n };
  }
  const json = JSON.stringify(tree);
  const back = varta.json.parse<Tree>(json);
  console.log(`${json.length} ${back.n} ${JSON.stringify(back) === json}`);
}
