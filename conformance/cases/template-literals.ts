function next(order: string[], value: string): string {
  order.push(value);
  return value;
}

export function main(): void {
  const order: string[] = [];
  const id = 42;
  console.log(`/api/v1/agents/${id}/activate`);
  console.log(`${next(order, "a")}-${next(order, "b")}-${next(order, "c")}`);
  console.log(String(order.length) + " " + order[0] + order[2]);
  console.log(`${0.1 + 0.2}|${-0}|${1e21}|${null}|${undefined}|${true}|${10n ** 20n}`);
  console.log(`crab \u{1F980} \`tick\` \${literal}`);
  const maybe: string | number | null = id > 40 ? 7 : null;
  console.log(`[${maybe}]`);
  console.log(`a${""}b`.length);
}
