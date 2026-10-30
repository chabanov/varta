function fib(n: number): number {
  if (n < 2) {
    return n;
  }
  return fib(n - 1) + fib(n - 2);
}
export function main(): void {
  console.log("старт");
  console.log(fib(10));
  console.log(2147483647 + 1);
  if (varta.env.has("APP_NAME")) {
    console.log("app " + varta.env.get("APP_NAME"));
  } else {
    console.log("no app name");
  }
  console.log(varta.result.status());
}
