interface Resource {
  id: string;
  kind: string;
}
interface Timestamped {
  created: number;
  updated?: number;
}
interface Agent extends Resource, Timestamped {
  kind: "agent";
  name: string;
}
interface Agent {
  tags: string[];
}
interface Page<T> {
  items: T[];
}
interface CursorPage<T> extends Page<T> {
  cursor: string | null;
}

function describe(resource: Resource): string {
  return resource.kind + "/" + resource.id;
}

function count<T>(page: CursorPage<T>): number {
  return page.items.length;
}

export function main(): void {
  const agent: Agent = {
    id: "a1",
    kind: "agent",
    name: "Ada",
    created: 10,
    tags: ["x", "y"],
  };
  console.log(describe({ id: agent.id, kind: agent.kind }));
  console.log(agent.name + " " + String(agent.created) + " " + String(agent.updated));
  console.log(agent.tags.length);
  console.log(JSON.stringify(agent));
  const page: CursorPage<Agent> = { items: [agent, agent], cursor: null };
  console.log(count(page));
  console.log(page.cursor === null);
}
