export function main(): void {
  console.log("before exit");
  varta.process.exit(3);
  console.log("unreachable");
}
