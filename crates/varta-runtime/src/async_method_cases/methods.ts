class Client {
  base: string;
  constructor(base: string) {
    this.base = base;
  }
  async request(path: string, attempt?: number): Promise<string> {
    console.log(`request ${path} ${attempt ?? 0}`);
    await null;
    return this.base + path;
  }
}
class Resource {
  client: Client;
  constructor(client: Client) {
    this.client = client;
  }
  async list(prefix?: string): Promise<string> {
    const first = await this.client.request("/list");
    console.log(`after ${first}`);
    return (prefix ?? "") + first;
  }
}
class Sub extends Resource {
  async list(prefix?: string): Promise<string> {
    console.log("sub before");
    const base = await super.list(prefix);
    console.log("sub after");
    return base + "!";
  }
  static async create(base: string): Promise<Sub> {
    await null;
    return new Sub(new Client(base));
  }
}
export async function main(): Promise<void> {
  const r = new Resource(new Client("api"));
  const p = r.list("x:");
  console.log("sync");
  console.log(await p);
  const s = await Sub.create("svc");
  const viaBase: Resource = s;
  console.log(await viaBase.list());
  console.log("done");
}
