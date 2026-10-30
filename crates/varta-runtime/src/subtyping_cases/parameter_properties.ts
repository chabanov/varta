// Constructor parameter properties declare and assign fields; a derived
// class assigns its own after `super(...)`.
class Transport {
  constructor(
    public readonly baseURL: string,
    private retries: number,
  ) {}
  describe(): string {
    return `${this.baseURL} x${this.retries}`;
  }
  retry(): number {
    this.retries -= 1;
    return this.retries;
  }
}
class Resource {
  constructor(protected readonly client: Transport) {}
}
class Runs extends Resource {
  constructor(
    client: Transport,
    readonly name: string,
  ) {
    super(client);
  }
  path(): string {
    return `${this.client.baseURL}/${this.name}`;
  }
}
export function main(): void {
  const transport = new Transport('https://api.test', 2);
  const runs = new Runs(transport, 'runs');
  console.log(`${transport.describe()} ${transport.retry()} ${runs.path()} ${runs.name}`);
  console.log(JSON.stringify(transport));
}
