// Promise.all, race, any and allSettled: job order and results.
async function delayed(label: string, ticks: number): Promise<string> {
  for (let i = 0; i < ticks; i++) {
    await null;
  }
  console.log(`settle ${label}`);
  return label;
}
async function failing(label: string, ticks: number): Promise<string> {
  for (let i = 0; i < ticks; i++) {
    await null;
  }
  console.log(`fail ${label}`);
  throw new Error(label);
}
async function all(): Promise<void> {
  const values = await Promise.all([delayed("a", 2), delayed("b", 0), delayed("c", 1)]);
  console.log(`all ${values.length} ${values[0]} ${values[1]} ${values[2]}`);
  try {
    await Promise.all([delayed("d", 3), failing("e", 1)]);
  } catch (error) {
    if (error instanceof Error) {
      console.log(`all rejected ${error.message}`);
    }
  }
}
async function race(): Promise<void> {
  const winner = await Promise.race([delayed("slow", 3), delayed("fast", 1)]);
  console.log(`race ${winner}`);
}
async function any(): Promise<void> {
  const first = await Promise.any([failing("x", 0), delayed("y", 2)]);
  console.log(`any ${first}`);
  try {
    await Promise.any([failing("p", 0), failing("q", 1)]);
  } catch (error) {
    if (error instanceof Error) {
      console.log(`any rejected ${error.message}`);
    }
  }
}
async function settled(): Promise<void> {
  const results = await Promise.allSettled([delayed("s", 1), failing("t", 0)]);
  for (let i = 0; i < results.length; i++) {
    const entry = results[i]!;
    console.log(`${entry.status} ${JSON.stringify(entry)}`);
  }
}
async function arrays(): Promise<void> {
  const empty: Promise<number>[] = [];
  const none = await Promise.all(empty);
  console.log(`empty all ${none.length}`);
  const tasks: Promise<string>[] = [];
  for (let i = 0; i < 3; i++) {
    tasks.push(delayed(`t${i}`, 3 - i));
  }
  const ordered = await Promise.all(tasks);
  console.log(ordered[0] + ordered[1] + ordered[2]);
}
export async function main(): Promise<void> {
  await all();
  await race();
  await any();
  await settled();
  await arrays();
  console.log("end");
}
