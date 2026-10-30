type JsonValue = string | number | boolean | null | JsonValue[] | { [key: string]: JsonValue };
interface UarpEvent {
  id?: string;
  event: string;
  data: string;
  json<T = JsonValue>(): T; // a generic method signature
}
type Mode = 'json' | 'text';
interface Spec {
  mode?: Mode;
  form?: object;
}

function makeEvent(fields: { id?: string; event: string; data: string }): UarpEvent {
  return {
    ...fields, // spread of a parameter with optional fields
    json<T = JsonValue>(): T {
      return JSON.parse(fields.data) as T;
    },
  };
}

function decode(mode: Mode): string {
  switch (mode) {
    case 'json':
      return 'J';
    case 'text':
      return 'T';
  } // exhaustive: no default needed
}

export function main(): void {
  const event = makeEvent({ event: 'run', data: '{"steps":[1,2]}' });
  const run = event.json<{ steps: number[] }>();
  const spec: Spec = {};
  const parsed: unknown = JSON.parse('[1,"a"]');
  let count = 0;
  if (Array.isArray(parsed)) count = parsed.length; // unknown[]
  let letters = '';
  for (const ch of 'hé😀') letters += `[${ch}]`;
  console.log(`${run.steps.length} ${decode(spec.mode ?? 'json')} ${count} ${letters} ${encodeURIComponent('a b')}`);
  // 2 J 2 [h][é][😀] a%20b
}
