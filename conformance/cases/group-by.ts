interface Sale {
  region: string;
  amount: number;
}
export function main(): void {
  const sales: Sale[] = [
    { region: "east", amount: 3 },
    { region: "west", amount: 5 },
    { region: "east", amount: 4 },
  ];
  console.log(JSON.stringify(Object.groupBy(sales, (sale) => sale.region)));
  const byParity = Map.groupBy([1, 2, 3, 4, 5], (n) => n % 2);
  console.log(`${byParity.get(1)!.join(",")} ${byParity.get(0)!.join(",")}`);
  console.log(JSON.stringify(Object.fromEntries([["b", 2], ["1", 1], ["a", 3]])));
  console.log(JSON.stringify(Object.fromEntries(new Map([["k", true]]))));
}
