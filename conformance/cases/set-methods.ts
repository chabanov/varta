export function main(): void {
  const a = new Set([1, 2, 3]);
  const b = new Set([3, 4]);
  console.log([...a.union(b)].join(","));
  console.log([...a.intersection(b)].join(","));
  console.log([...a.difference(b)].join(","));
  console.log([...a.symmetricDifference(b)].join(","));
  console.log(`${String(new Set([1]).isSubsetOf(a))} ${String(a.isSupersetOf(b))} ${String(a.isDisjointFrom(new Set([9])))}`);
}
