// Promises held in dictionaries and mixed records (v35 `promise_values` with
// `mixed_index_records`), next to `JSON.parse` producing `unknown`.
async function double(n: number): Promise<number> {
  await null;
  return n * 2;
}
interface Batch {
  label: string;
  [key: string]: string | Promise<number>;
}
export async function main(): Promise<void> {
  const pending: Record<string, Promise<number>> = {};
  pending["a"] = double(1);
  pending.b = double(2);
  for (const key in pending) {
    const job = pending[key];
    if (job !== undefined) {
      const value = await job;
      console.log(`${key}=${value}`);
    }
  }
  const all = await Promise.all(Object.values(pending));
  console.log(`${all[0]},${all[1]}`);
  const batch: Batch = { label: "jobs", x: double(5) };
  batch.y = double(6);
  for (const key in batch) {
    const entry = batch[key];
    if (entry !== undefined && typeof entry !== "string") {
      const value = await entry;
      console.log(`${batch.label}.${key}=${value}`);
    }
  }
  const copy: Record<string, Promise<number>> = { ...pending, c: double(3) };
  const settled = await Promise.allSettled(Object.values(copy));
  console.log(JSON.stringify(settled));
  const parsed: unknown = JSON.parse('{"jobs":[1,2],"label":"p"}');
  if (typeof parsed === "object" && parsed !== null && "jobs" in parsed) {
    console.log(JSON.stringify(parsed));
  }
}
