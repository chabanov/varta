// Branded primitives, `typeof` of local bindings and `keyof`/indexed
// access/mapped types over class instances and tuples.
type UserId = string & { readonly __brand: "UserId" };
type Meters = number & { __unit: "m" };
class Account {
  owner: string = "ada";
  balance: number = 5;
  describe(): string {
    return `${this.owner}:${this.balance}`;
  }
  get doubled(): number {
    return this.balance * 2;
  }
}
type AccountKey = keyof Account;
type Draft = Partial<Account>;
type PairKey = keyof [string, number];

function asId(raw: string): UserId {
  return raw as UserId;
}
function twice(length: Meters): number {
  return length * 2;
}
function sum(values: number[]): number {
  let total = 0;
  for (const value of values) total += value;
  return total;
}

export function main(): void {
  const id = asId("u1");
  const length = 3 as Meters;
  const plain: string = id;
  console.log(`${id.toUpperCase()} ${twice(length)} ${plain.length}`);
  const point = { x: 1, y: 2 };
  const copy: typeof point = { x: 3, y: 4 };
  const keys: (keyof typeof point)[] = ["x", "y"];
  let total = 0;
  for (const key of keys) {
    total += copy[key];
  }
  const label = "total";
  const shown: typeof label = "total";
  const list = [1, 2, 3];
  const more: typeof list = [4];
  console.log(`${shown} ${total} ${sum(more)}`);
  const key: AccountKey = "describe";
  const getter: AccountKey = "doubled";
  const draft: Draft = { balance: 9 };
  const pairKey: PairKey = "length";
  const index: PairKey = 1;
  const method: Account["describe"] = () => "custom";
  console.log(`${key} ${getter} ${draft.balance} ${draft.owner} ${pairKey} ${index} ${method()} ${new Account().describe()}`);
}
