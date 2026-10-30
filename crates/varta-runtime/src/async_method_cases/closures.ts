class Api {
  name: string;
  calls: number;
  constructor(name: string) {
    this.name = name;
    this.calls = 0;
  }
  async request(path: string): Promise<string> {
    this.calls = this.calls + 1;
    await null;
    return `${this.name}${path}#${this.calls}`;
  }
  async withRetry(path: string, attempts: number): Promise<string> {
    let last = "";
    const once = async (n: number): Promise<string> => {
      const value = await this.request(`${path}?try=${n}`);
      last = value;
      return value;
    };
    for (let i = 0; i < attempts; i++) {
      await once(i);
    }
    return last;
  }
}
async function retry<T>(fn: () => Promise<T>, times: number): Promise<T> {
  let result = await fn();
  for (let i = 1; i < times; i++) {
    result = await fn();
  }
  return result;
}
export async function main(): Promise<void> {
  const api = new Api("svc");
  let counter = 0;
  const bump = async (by: number) => {
    counter = counter + by;
    await null;
    counter = counter * 2;
    return counter;
  };
  const p = bump(1);
  counter = counter + 10;
  console.log(`sync ${counter}`);
  const bumped = await p;
  console.log(`bumped ${bumped} ${counter}`);
  console.log(await api.withRetry("/a", 2));
  const g = async function (x: number): Promise<number> {
    await null;
    return x * 3;
  };
  console.log(await g(4));
  console.log(await retry(async () => api.request("/r"), 3));
  const doubled = await Promise.resolve(5).then(async (v) => {
    await null;
    return v * 2;
  });
  console.log(doubled);
  const e = async (): Promise<void> => {
    throw new Error("boom");
  };
  try {
    await e();
  } catch (error) {
    if (error instanceof Error) {
      console.log(`caught ${error.message}`);
    }
  }
}
