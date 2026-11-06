// Mapped types: homomorphic modifiers, `as` remapping, +/- modifiers,
// literal key sets, string keys, tuples and arrays.
interface User {
  readonly id: number;
  name: string;
  email?: string;
}
type Getters<T> = {
  [K in keyof T as `get${Capitalize<string & K>}`]: () => T[K];
};
type Nullable<T> = { [K in keyof T]: T[K] | null };
type Mutable<T> = { -readonly [K in keyof T]: T[K] };
type Concrete<T> = { [K in keyof T]-?: T[K] };
type Loose<T> = { +readonly [K in keyof T]+?: T[K] };
type OnlyStrings<T> = { [K in keyof T as T[K] extends string ? K : never]: T[K] };
type Box<T> = { [K in "value" | "label"]: T };
type Flags = { [K in "a" | "b" as `${K}Enabled`]: boolean };
type Counts = { [K in string]: number };

function describe(user: Concrete<User>): string {
  return `${user.id}:${user.name}:${user.email}`;
}

export function main(): void {
  const user: User = { id: 1, name: "Ada" };
  const getters: Getters<User> = {
    getId: () => user.id,
    getName: () => user.name,
  };
  console.log(`${getters.getId()} ${getters.getName()} ${getters.getEmail === undefined}`);
  const nullable: Nullable<User> = { id: null, name: "x" };
  console.log(`${nullable.id} ${nullable.name} ${nullable.email}`);
  const mutable: Mutable<User> = { id: 2, name: "B" };
  mutable.id = 3;
  console.log(`${mutable.id}`);
  console.log(describe({ id: 4, name: "C", email: "c@example.org" }));
  const loose: Loose<User> = {};
  console.log(`${loose.id} ${loose.name}`);
  const strings: OnlyStrings<User> = { name: "only" };
  console.log(Object.keys(strings).join(","));
  const box: Box<number> = { value: 1, label: 2 };
  const flags: Flags = { aEnabled: true, bEnabled: false };
  console.log(`${box.value + box.label} ${flags.aEnabled} ${flags.bEnabled}`);
  const counts: Counts = {};
  counts["x"] = 1;
  counts["y"] = 2;
  console.log(`${counts["x"] + counts["y"]}`);
  const pair: Nullable<[number, string]> = [null, "p"];
  const list: Nullable<number[]> = [1, null, 3];
  console.log(`${pair[0]} ${pair[1]} ${list.join("|")}`);
}
