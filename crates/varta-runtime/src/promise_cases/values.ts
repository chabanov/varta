// Promise values of every admitted kind: void, unions, records, arrays and
// class instances; async functions as function values and promise-typed
// parameters, results and closure captures; then(undefined, onRejected).
class Account {
  name: string;
  balance: number;
  constructor(name: string, balance: number) {
    this.name = name;
    this.balance = balance;
  }
  describe(): string {
    return this.name + "=" + String(this.balance);
  }
}
interface Summary {
  count: number;
  names: string[];
}
async function open(name: string): Promise<Account> {
  await null;
  return new Account(name, name.length);
}
async function maybe(flag: boolean): Promise<number | undefined> {
  await flag;
  return flag ? 1 : undefined;
}
function later(): Promise<void> {
  return new Promise<void>((resolve) => {
    console.log("void executor");
    resolve();
  });
}
async function twice(n: number): Promise<number> {
  return n * 2;
}
function apply(f: (n: number) => Promise<number>, n: number): Promise<number> {
  return f(n);
}
export async function main(): Promise<void> {
  const account = await open("alice");
  console.log(account.describe());
  const summary: Summary = await Promise.resolve<Summary>({ count: 2, names: ["a", "b"] });
  console.log(String(summary.count) + " " + summary.names[1]);
  const none = await maybe(false);
  console.log(String(none === undefined));
  const some = await maybe(true);
  console.log(String(some));
  await later();
  console.log("void resolved");
  const doubled = await apply(twice, 21);
  console.log(String(doubled));
  const pending = Promise.reject<string>("nope");
  const recovered = await pending.then(undefined, (reason) => {
    if (typeof reason === "string") {
      return "recovered " + reason;
    }
    return "other";
  });
  console.log(recovered);
  const captured = Promise.resolve(3);
  const read = (): Promise<number> => captured.then((v) => v + 1);
  const next = await read();
  console.log(String(next));
  const handled = await Promise.resolve(1).catch(() => 0);
  console.log(String(handled));
}
