interface Pending {
  label: string;
  task: Promise<number>;
}
async function value(n: number): Promise<number> {
  await null;
  return n * 2;
}
export async function main(): Promise<void> {
  const tasks: Promise<number>[] = [value(1), value(2)];
  tasks.push(value(3));
  let total = 0;
  for (const task of tasks) {
    const v = await task;
    total = total + v;
  }
  console.log(`total ${total}`);
  const record: Pending = { label: "r", task: value(5) };
  const got = await record.task;
  console.log(`${record.label} ${got}`);
  const maybe: Promise<number> | undefined = tasks.length > 1 ? tasks[0] : undefined;
  if (maybe !== undefined) {
    const first = await maybe;
    console.log(`maybe ${first}`);
  }
  const same = tasks[1] === tasks[1];
  const other = tasks[0] === tasks[1];
  console.log(`${same} ${other} ${typeof tasks[0]}`);
  const held: unknown = tasks[2];
  console.log(typeof held);
  if (held === tasks[2]) {
    console.log("held");
  }
  const byName: Record<string, Promise<number>> = { a: value(7) };
  const seven = await byName["a"]!;
  console.log(`dict ${seven}`);
}
