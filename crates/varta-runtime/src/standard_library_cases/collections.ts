// ES2025 Set methods (ECMA-262 §24.2.4), Map.groupBy (§24.1.2.1),
// Object.groupBy (§20.1.2.12) and Object.fromEntries (§20.1.2.7).
interface Order {
  customer: string;
  total: number;
}

function list(set: Set<number>): string {
  return `{${[...set].join(",")}}`;
}

export function main(): void {
  const odds = new Set([1, 3, 5, 7, 9]);
  const squares = new Set([1, 4, 9]);
  const small = new Set([9, 1]);
  console.log(list(odds.union(squares)));
  console.log(list(odds.intersection(squares)));
  console.log(list(odds.intersection(small)));
  console.log(list(small.intersection(odds)));
  console.log(list(odds.difference(squares)));
  console.log(list(squares.difference(odds)));
  console.log(list(odds.symmetricDifference(squares)));
  console.log(`${String(small.isSubsetOf(odds))} ${String(odds.isSubsetOf(small))}`);
  console.log(`${String(odds.isSupersetOf(small))} ${String(small.isSupersetOf(odds))}`);
  console.log(`${String(odds.isDisjointFrom(new Set([2, 4])))} ${String(odds.isDisjointFrom(squares))}`);
  console.log(`${odds.size} ${squares.size}`);
  const words = new Set(["a", "b"]);
  console.log([...words.union(new Set(["c", "a"]))].join(""));

  const orders: Order[] = [
    { customer: "ann", total: 10 },
    { customer: "bob", total: 5 },
    { customer: "ann", total: 7 },
  ];
  const byCustomer = Map.groupBy(orders, (order) => order.customer);
  for (const [customer, group] of byCustomer) {
    console.log(`${customer}: ${group.map((order) => order.total).join("+")}`);
  }
  const bySize = Map.groupBy([1, 2, 3, 4, 5], (value, index) => (value + index) % 3);
  console.log([...bySize.keys()].join(","));
  console.log(JSON.stringify(Object.groupBy(orders, (order) => order.customer)));
  console.log(JSON.stringify(Object.groupBy([1, 2, 3, 10, 21], (value) => (value > 2 ? "big" : value))));
  const grouped = Object.groupBy(["apple", "avocado", "banana"], (word) => word[0]!);
  console.log(Object.keys(grouped).join(","));

  console.log(JSON.stringify(Object.fromEntries([["b", 1], ["2", 2], ["a", 3], ["1", 4]])));
  const pairs: [string, boolean][] = [["x", true], ["y", false]];
  console.log(JSON.stringify(Object.fromEntries(pairs)));
  const map = new Map<string, number>([["k", 1], ["j", 2]]);
  const object = Object.fromEntries(map);
  console.log(`${JSON.stringify(object)} ${object["k"]}`);
  console.log(JSON.stringify(Object.fromEntries([[1, "one"], [2, "two"]])));
}
