// Closures and generator frames retaining value graphs across collection.
interface Node {
  name: string;
  children: string[];
  weight?: number;
}
function counter(start: Node): () => string {
  let calls = 0;
  return () => {
    calls = calls + 1;
    start.children.push("c" + String(calls));
    return JSON.stringify(start);
  };
}
function* walk(nodes: Node[]): Generator<Node, number, string> {
  let seen = 0;
  for (let index = 0; index < nodes.length; index++) {
    const node = nodes[index];
    if (node === undefined) {
      continue;
    }
    let garbage = "";
    for (let i = 0; i < 24; i++) {
      garbage = garbage + String(i);
    }
    const reply = yield { name: node.name + garbage.length, children: node.children.slice(0) };
    seen = seen + reply.length;
  }
  return seen;
}
export function main(): void {
  const next = counter({ name: "root", children: [] });
  next();
  console.log(next());
  const nodes: Node[] = [
    { name: "a", children: ["x"] },
    { name: "b", children: [], weight: 2 },
  ];
  const it = walk(nodes);
  let step = it.next("");
  while (step.done === false) {
    console.log(JSON.stringify(step.value));
    step = it.next("ok!");
  }
  const last = step.value;
  if (typeof last === "number") {
    console.log(String(last));
  }
  const total = nodes.reduce((sum: number, node: Node) => sum + (node.weight ?? 1), 0);
  console.log(String(total) + " " + JSON.stringify(nodes.map((n: Node) => n.name)));
}
