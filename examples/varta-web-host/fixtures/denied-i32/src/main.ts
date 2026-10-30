export function main(): void {
  console.log(varta.time.now());
  console.log(varta.result.status());
  varta.env.print("APP_NAME");
  console.log(varta.result.status());
  varta.process.exit(3);
  console.log(varta.result.status());
  console.log("still running");
}
