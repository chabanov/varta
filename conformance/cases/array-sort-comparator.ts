interface Task {
  name: string;
  priority: number;
}
export function main(): void {
  const calls: string[] = [];
  const values = [5, 1, 4, 2, 3];
  values.sort((a, b) => {
    calls.push(`${a}:${b}`);
    return a - b;
  });
  console.log(values.join(","));
  console.log(calls.join(" "));
  const tasks: Task[] = [
    { name: "b", priority: 2 },
    { name: "a", priority: 1 },
    { name: "c", priority: 2 },
  ];
  const ordered = tasks.toSorted((x, y) => x.priority - y.priority);
  console.log(ordered.map((task) => task.name).join(","));
  console.log(tasks.map((task) => task.name).join(","));
  console.log([10, 9, 1, 100].sort().join(","));
}
