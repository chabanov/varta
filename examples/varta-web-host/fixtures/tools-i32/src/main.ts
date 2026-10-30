export function echo(value: string): string {
  return value;
}
export function add(a: number, b: number): number {
  return a + b;
}
export function negate(flag: boolean): boolean {
  return !flag;
}
export function shout(value: string): void {
  console.log(value + "!");
  console.log(42);
}
export function appName(): string {
  return varta.env.get("APP_NAME");
}
export function main(): void {}
varta.tool({ name: "echo", description: "Return the text unchanged" }, echo);
varta.tool({ name: "add", description: "Add two integers with i32 wrap-around" }, add);
varta.tool({ name: "negate", description: "Negate a boolean" }, negate);
varta.tool({ name: "shout", description: "Log the text with an exclamation mark" }, shout);
varta.tool({ name: "app.name", description: "Read APP_NAME from the environment" }, appName);
