export function main(): void {
  const found = /(\d+)-(x)?(\w+)/.exec("id 42-abc");
  if (found !== null) {
    console.log(`${found.length} ${found.index} ${String(Array.isArray(found))}`);
    console.log(found.slice(1).map((part) => part ?? "~").join("|"));
    console.log(`${found.indexOf("abc")} ${found.at(-1)} ${JSON.stringify(found)}`);
    found.push("tail");
    console.log(`${found.length} ${found.join(",")}`);
  }
}
