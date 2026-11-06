// Promise.allSettled / any / race / all over array literals that mix
// Promise.reject with fulfilled promises (ECMA-262 §27.2.4), and
// Promise.withResolvers (§27.2.4.8).
export async function main(): Promise<void> {
  const settled = await Promise.allSettled([Promise.resolve(1), Promise.reject(new Error("e")), Promise.resolve(3)]);
  console.log(settled.map((entry) => entry.status).join(","));
  for (const entry of settled) {
    if (entry.status === "fulfilled") {
      console.log(`value ${String(entry.value)}`);
    } else {
      console.log(`reason ${(entry.reason as Error).message}`);
    }
  }
  const first = await Promise.any([Promise.reject(new Error("x")), Promise.resolve(2), Promise.resolve(5)]);
  console.log(first);
  try {
    await Promise.all([Promise.resolve("a"), Promise.reject(new TypeError("boom"))]);
  } catch (error) {
    console.log(`${(error as Error).name}: ${(error as Error).message}`);
  }
  const winner = await Promise.race([Promise.resolve("fast"), Promise.reject(new Error("slow"))]);
  console.log(winner);
  const { promise, resolve } = Promise.withResolvers<number>();
  void Promise.resolve(40).then((value) => resolve(value + 2));
  console.log(await promise);
}
