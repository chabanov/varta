// Language guide example: promise values and combinators (v35).
interface Download {
  name: string;
  done: Promise<number>;
}
async function fetchSize(name: string, ticks: number): Promise<number> {
  for (let i = 0; i < ticks; i++) {
    await null;
  }
  if (name === "broken") {
    throw new Error(`cannot fetch ${name}`);
  }
  return name.length * 100;
}
export async function main(): Promise<void> {
  const downloads: Download[] = [
    { name: "a", done: fetchSize("a", 2) },
    { name: "bb", done: fetchSize("bb", 1) },
  ];
  const pending: Promise<number>[] = [];
  for (let i = 0; i < downloads.length; i++) {
    pending.push(downloads[i]!.done);
  }
  console.log(pending[0]); // Promise { <pending> }
  const sizes = await Promise.all(pending);
  console.log(`${sizes[0]} ${sizes[1]}`); // 100 200
  const fastest = await Promise.race([fetchSize("slow", 3), fetchSize("quick", 1)]);
  console.log(fastest); // 500
  const results = await Promise.allSettled([fetchSize("ok", 0), fetchSize("broken", 0)]);
  console.log(JSON.stringify(results)); // [{"status":"fulfilled","value":200},{"status":"rejected","reason":{}}]
  const cached: Promise<number> | number = sizes[0]!;
  console.log(await cached); // 100
}
