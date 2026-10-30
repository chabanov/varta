async function fail(): Promise<void> {
  throw new Error("boom");
}
export function main(): void {
  console.log("start");
  fail();
}
