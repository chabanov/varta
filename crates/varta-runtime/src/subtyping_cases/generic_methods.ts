type JsonPrimitive = string | number | boolean | null;
type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
interface UarpEvent {
  id?: string;
  event: string;
  data: string;
  retry?: number;
  json<T = JsonValue>(): T;
}
interface Run {
  id: string;
  steps: number;
}
interface Box {
  wrap<T>(value: T): T;
  label: string;
}

function makeEvent(fields: { id?: string; event: string; data: string; retry?: number }): UarpEvent {
  return {
    id: fields.id,
    event: fields.event,
    data: fields.data,
    retry: fields.retry,
    json<T = JsonValue>(): T {
      return JSON.parse(fields.data) as T;
    },
  };
}

export function main(): void {
  const e = makeEvent({ event: "run", data: '{"id":"r1","steps":3}' });
  const run = e.json<Run>();
  console.log(`${run.id} ${run.steps}`);
  const value = e.json();
  console.log(JSON.stringify(value));
  const raw = e.json<unknown>();
  console.log(typeof raw);
  const box: Box = {
    label: "b",
    wrap<T>(value: T): T {
      return value;
    },
  };
  console.log(`${box.wrap<number>(4) + 1} ${box.wrap("s")}`);
}
