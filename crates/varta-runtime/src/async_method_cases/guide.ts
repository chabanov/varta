// Language guide example: async methods and async closures (v35).
class Client {
  base: string;
  constructor(base: string) {
    this.base = base;
  }
  async request(path: string, retries?: number): Promise<string> {
    await null; // one microtask
    return `${this.base}${path} (retries ${retries ?? 0})`;
  }
}
class Pages {
  client: Client;
  constructor(client: Client) {
    this.client = client;
  }
  async list(page?: number): Promise<string> {
    return await this.client.request(`/pages/${page ?? 1}`);
  }
}
async function withRetry<T>(run: () => Promise<T>, attempts: number): Promise<T> {
  for (let i = 1; i < attempts; i++) {
    try {
      return await run();
    } catch (error) {
      console.log(`attempt ${i} failed`);
    }
  }
  return await run();
}
export async function main(): Promise<void> {
  const pages = new Pages(new Client("api"));
  console.log(await pages.list()); // api/pages/1 (retries 0)
  let calls = 0;
  const text = await withRetry(async () => {
    calls++; // shared with main across await
    if (calls < 2) {
      throw new Error("busy");
    }
    return pages.list(calls);
  }, 3); // attempt 1 failed
  console.log(`${text} after ${calls} calls`); // api/pages/2 (retries 0) after 2 calls
}
