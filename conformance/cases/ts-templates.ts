// Template literal types and the intrinsic string manipulation types.
type Event = "click" | "key";
type Handler = `on${Capitalize<Event>}`;
type Size = "sm" | "lg";
type Class = `btn-${Size}-${1 | 2}`;
type Shout<T extends string> = Uppercase<T>;
type Whisper = Lowercase<"HeLLo">;
type Lower = Uncapitalize<"Name">;
type Getter<K extends string> = `get${Capitalize<K>}`;
type Trim<S extends string> = S extends ` ${infer R}` ? Trim<R> : S;
type Split<S extends string> = S extends `${infer H}.${infer T}` ? [H, ...Split<T>] : [S];
type Route<S extends string> = S extends `/${infer Head}/${string}` ? Head : "root";
type Flag = `${boolean}`;

function handle(name: Handler): string {
  return name;
}

export function main(): void {
  const handlers: Handler[] = ["onClick", "onKey"];
  console.log(`${handlers.join(",")} ${handle("onKey")}`);
  const classes: Class[] = ["btn-sm-1", "btn-lg-2"];
  console.log(classes.join(" "));
  const shout: Shout<"hey" | "you"> = "YOU";
  const whisper: Whisper = "hello";
  const lower: Lower = "name";
  const getter: Getter<"size"> = "getSize";
  console.log(`${shout} ${whisper} ${lower} ${getter}`);
  const trimmed: Trim<"   x"> = "x";
  const parts: Split<"a.b.c"> = ["a", "b", "c"];
  const head: Route<"/users/42"> = "users";
  const root: Route<"plain"> = "root";
  const flags: Flag[] = ["true", "false"];
  console.log(`${trimmed} ${parts.join("+")} ${parts.length} ${head} ${root} ${flags.join("")}`);
}
