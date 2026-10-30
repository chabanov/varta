export function main(): void {
  console.log("before");
  setTimeout(() => console.log("never"), 6000);
}
