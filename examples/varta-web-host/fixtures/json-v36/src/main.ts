interface Item {
  id: string;
  count: number;
}
export function main(): void {
  const item: Item = { id: "a1", count: 2 };
  console.log(JSON.stringify(item));
}
