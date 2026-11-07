// Pattern template literal types at run time (K9 A step 4): a checked
// assertion of a string to a pattern type tests it against the pattern, as
// the compile-time relation does (values that do not match throw TypeError;
// see the deviation test).
type Handler = `on${string}`;
type Size = `${number}px`;
type Id = `id-${bigint}`;
type Shout = Uppercase<string>;
type Event = `${Capitalize<string>}:${number}`;
type Theme = "auto" | `#${string}`;

interface Config {
  handler: Handler;
  sizes: Size[];
  id?: Id;
}

function parse(text: string): string {
  const config = JSON.parse(text) as Config;
  return `${config.handler} ${config.sizes.join(",")} ${config.id ?? "-"}`;
}

function sizes(values: unknown[]): string {
  return values.map((value) => value as Size).join(" ");
}

export function main(): void {
  const handlers: unknown[] = ["onClick", "on", "onon"];
  console.log(handlers.map((value) => value as Handler).join(" "));
  console.log(sizes(["12px", "1e3px", " px", "0x1fpx", "-.5px"]));
  const id = "id-42" as unknown as Id;
  const shout = "HELLO WORLD" as unknown as Shout;
  const event = "Click:3" as unknown as Event;
  console.log(`${id} ${shout} ${event}`);
  const ids: unknown[] = ["id-0x1F", "id-0", "id--5", "id-0b101"];
  console.log(ids.map((value) => value as Id).join(" "));
  const events: unknown[] = ["Click:1", "Ab:3", "É: 2", "1:1"];
  console.log(events.map((value) => value as Event).join(" "));
  const themes: unknown[] = ["auto", "#fff", "#"];
  console.log(themes.map((value) => value as Theme).join(" "));
  console.log(parse('{"handler":"onKey","sizes":["1px","2.5px"]}'));
  console.log(parse('{"handler":"onA","sizes":[],"id":"id-7"}'));
}
