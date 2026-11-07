// Index signatures of pattern key types (K9 A step 4): `Record` and mapped
// types over a template literal type admit only the keys the pattern
// describes, as in tsc; the values are a dictionary like `Record<string, T>`.
type Handlers = Record<`on${string}`, number>;
type Data = { [K in `data-${string}`]: string };
type Props = Record<"id" | `aria-${string}`, string>;
interface Listeners {
  [event: `on${string}`]: (detail: string) => string;
}

function total(handlers: Handlers): number {
  let sum = 0;
  for (const key in handlers) {
    sum += handlers[key as `on${string}`];
  }
  return sum;
}

function attribute(data: Data, name: `data-${string}`): string {
  return data[name] ?? "none";
}

export function main(): void {
  const handlers: Handlers = { onClick: 1, onKey: 2 };
  handlers.onFocus = 3;
  handlers["onBlur"] = 4;
  delete handlers.onKey;
  console.log(`${handlers.onClick} ${Object.keys(handlers).join(",")} ${total(handlers)} ${"onBlur" in handlers}`);
  const plain: Record<string, number> = { a: 1 };
  const merged: Handlers = { onA: 5, ...plain };
  const back: Record<string, number> = merged;
  console.log(`${Object.entries(back).map(([key, value]) => `${key}=${value}`).join("&")}`);
  const data: Data = { "data-x": "1" };
  data["data-y"] = "2";
  console.log(`${attribute(data, "data-x")} ${attribute(data, "data-z")} ${JSON.stringify(data)}`);
  const props: Props = { id: "main", "aria-label": "Main" };
  props["aria-hidden"] = "true";
  console.log(`${props.id} ${props["aria-label"]} ${Object.keys(props).length}`);
  const keys: keyof Props = "aria-live";
  console.log(keys);
  const listeners: Listeners = { onSave: (detail) => `saved ${detail}` };
  const save = listeners.onSave;
  console.log(save === undefined ? "none" : save("doc"));
}
