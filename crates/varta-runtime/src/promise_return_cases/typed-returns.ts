async function decode(response: Response, mode: string): Promise<unknown> {
  if (mode === "text") return response.text();
  if (mode === "number") return Promise.resolve(7);
  if (mode === "record") return Promise.resolve({ label: "record" });
  if (mode === "void") return Promise.resolve();
  return "plain";
}

class Transport {
  async request<T>(response: Response, mode: string): Promise<T> {
    return (await decode(response, mode)) as T;
  }
}

async function replaced(mode: number): Promise<unknown> {
  try {
    return Promise.resolve("original");
  } finally {
    console.log("finally", mode);
    if (mode === 1) return 9;
    await Promise.resolve();
    if (mode === 2) return Promise.resolve(8);
    if (mode === 3) return Promise.resolve<unknown>("same target");
  }
}

async function rejected(): Promise<unknown> {
  try {
    return Promise.reject<string>(new Error("typed rejection"));
  } finally {
    console.log("reject finally");
  }
}

async function conditional(flag: boolean): Promise<unknown> {
  return flag ? Promise.resolve("yes") : Promise.resolve(2);
}

async function ordered(): Promise<unknown> {
  try {
    return Promise.resolve("adopted");
  } finally {
    console.log("order finally");
  }
}

export async function main(): Promise<void> {
  const transport = new Transport();
  console.log(await transport.request<string>(new Response("fixture-ok"), "text"));
  console.log(await decode(new Response(""), "number"));
  const record = (await decode(new Response(""), "record")) as { label: string };
  console.log(record.label);
  console.log(await decode(new Response(""), "void"));
  console.log(await decode(new Response(""), "other"));
  for (let mode = 0; mode < 4; mode++) console.log(await replaced(mode));
  try { await rejected(); } catch (error) {
    if (error instanceof Error) console.log(error.message);
  }
  console.log(await conditional(true), await conditional(false));
  const arrow: () => Promise<unknown> = async (): Promise<unknown> => (Promise.resolve("arrow"));
  console.log(await arrow());
  const result = ordered();
  const ticks = Promise.resolve().then(() => console.log("tick 1"))
    .then(() => console.log("tick 2")).then(() => console.log("tick 3"))
    .then(() => console.log("tick 4"));
  console.log("after call");
  console.log(await result);
  await ticks;
  const hidden: unknown = Promise.resolve("hidden");
  try { await hidden; } catch (error) {
    if (error instanceof TypeError) console.log("unknown promise refused");
  }
}
