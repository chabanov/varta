interface Order {
  customer: string;
  total: number;
  placed: Date;
}
function* ids(): Generator<number, void, undefined> {
  for (let id = 1; ; id++) yield id;
}
export function main(): void {
  const orders: Order[] = [
    { customer: "ann", total: 30, placed: new Date(2024, 1, 29) },
    { customer: "bob", total: 12, placed: new Date(2024, 0, 15) },
    { customer: "ann", total: 7, placed: new Date(2024, 2, 1) },
  ];
  const byTotal = orders.toSorted((a, b) => b.total - a.total);
  console.log(byTotal.map((o) => o.total).join(",")); // 30,12,7
  const groups = Map.groupBy(orders, (o) => o.customer);
  console.log([...groups.keys()].join(",")); // ann,bob
  console.log(JSON.stringify(Object.groupBy(orders, (o) => o.customer)["ann"]!.map((o) => o.total))); // [30,7]
  console.log(ids().filter((id) => id % 2 === 0).map((id) => id * 10).take(3).toArray().join(",")); // 20,40,60
  console.log([...new Set(["a", "b"]).union(new Set(["c"]))].join("")); // abc
  console.log(JSON.stringify({ first: byTotal[0]!.customer, totals: [30, 12] }, null, 1).split("\n").length); // 7
  console.log(orders[0]!.placed.toUTCString()); // Thu, 29 Feb 2024 00:00:00 GMT
  console.log(Math.max(...orders.map((o) => o.total))); // 30
}
