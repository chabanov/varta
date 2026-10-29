interface CursorPage<T> {
  items: T[];
  cursor: string | null;
}
type Pair<A, B = A> = { first: A; second: B };
interface Agent {
  id: string;
  score: number;
}
type AgentKey = keyof Agent;

function first<T>(items: T[], fallback: T): T {
  const item = items[0];
  return item === undefined ? fallback : item;
}

function collect<T>(pages: CursorPage<T>[]): T[] {
  const out: T[] = [];
  for (const page of pages) {
    if (page === undefined) continue;
    for (const item of page.items) {
      if (item !== undefined) out.push(item);
    }
  }
  return out;
}

function score(agent: Agent): Agent["score"] {
  return agent.score;
}

function largest<T>(items: T[], fallback: T, rank: (item: T) => number): T {
  let best = fallback;
  for (const item of items) {
    if (item !== undefined && rank(item) > rank(best)) best = item;
  }
  return best;
}

function swap<A, B>(pair: Pair<A, B>): Pair<B, A> {
  return { first: pair.second, second: pair.first };
}

export function main(): void {
  console.log(first([3, 4], 0));
  console.log(first<string>([], "none"));
  const pages: CursorPage<Agent>[] = [
    { items: [{ id: "a1", score: 2 }], cursor: "next" },
    { items: [{ id: "a2", score: 5 }, { id: "a3", score: 1 }], cursor: null },
  ];
  const agents = collect(pages);
  console.log(agents.length);
  const best = largest(agents, { id: "none", score: 0 }, (agent: Agent): number => score(agent));
  console.log(best.id + ":" + String(score(best)));
  const key: AgentKey = "score";
  console.log(key);
  const swapped = swap({ first: "x", second: 7 });
  console.log(swapped.first + 1);
  const same: Pair<boolean> = { first: true, second: false };
  console.log(same.first && !same.second);
  const flags: Record<"read" | "write", boolean> = { read: true, write: false };
  const patch: Partial<Agent> = { score: 9 };
  console.log(JSON.stringify(flags) + JSON.stringify(patch));
}
