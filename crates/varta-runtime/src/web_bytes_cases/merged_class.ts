// V38 declaration merging of a class with a same-named interface whose
// members the constructor provides with `Object.assign(this, …)`, the
// pilot's client pattern. Node 24 prints the same lines (`node
// merged_class.ts` after appending `main();`).
class Transport {
  readonly base: string;
  constructor(base: string) {
    this.base = base;
  }
  url(path: string): string {
    return this.base + path;
  }
}
class AgentsResource {
  private readonly transport: Transport;
  constructor(transport: Transport) {
    this.transport = transport;
  }
  list(): string {
    return `GET ${this.transport.url('/agents')}`;
  }
}
class RunsResource {
  private readonly transport: Transport;
  constructor(transport: Transport) {
    this.transport = transport;
  }
  get(id: string): string {
    return `GET ${this.transport.url('/runs/' + id)}`;
  }
}
interface Resources {
  readonly agents: AgentsResource;
  readonly runs: RunsResource;
}
interface Helpers {
  describe: () => string;
  retries?: number;
}
function createResources(transport: Transport): Resources {
  return { agents: new AgentsResource(transport), runs: new RunsResource(transport) };
}

export interface Client extends Resources, Helpers {}

export class Client {
  readonly transport: Transport;
  constructor(base: string) {
    this.transport = new Transport(base);
    const resources = createResources(this.transport);
    Object.assign(this, {
      agents: resources.agents,
      runs: resources.runs,
      describe: (): string => `client of ${base}`,
      retries: 2,
    });
  }
  get baseURL(): string {
    return this.transport.base;
  }
}

function use(client: Resources): string {
  return client.runs.get('r1');
}

export function main(): void {
  const client = new Client('https://api.test');
  console.log(client.agents.list());
  console.log(use(client));
  console.log(`${client.describe()} ${client.retries} ${client.baseURL}`);
  console.log(`${client instanceof Client}`);
}
