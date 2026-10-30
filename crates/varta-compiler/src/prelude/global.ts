// Engine prelude of checked-f64-utf16-v38 (`web_bytes`): the typed global
// object. `globalThis` evaluates to a `GlobalObject` whose members are the
// globals a program may read through it; any other member is a compile error,
// never `any`. The compiler loads the prelude when a module names
// `globalThis`; see docs/architecture.md (Web bytes).
//
// `process.env` is a snapshot dictionary of the set environment variables
// whose names the `env.get` grants list exactly (`varta.env.snapshot`,
// capability `env.get`); a wildcard grant lists nothing, so the environment
// is never enumerated. `process.env.NAME` with a static key reads that one
// variable through `env.get` when it runs (`environmentValue`), so wildcard
// grants are usable by name. A variable outside the grants reads as
// `undefined`, exactly like an unset one. Writes change only the snapshot.

// The `crypto` global (Web Crypto's random functions).
export class Crypto {
  getRandomValues(array: Uint8Array): Uint8Array {
    return crypto.getRandomValues(array);
  }

  randomUUID(): string {
    return crypto.randomUUID();
  }
}

// The `process` global: its environment only.
export class Process {
  get env(): Record<string, string | undefined> {
    const text = varta.env.snapshot('');
    const env: Record<string, string | undefined> = {};
    if (text === '') {
      return env;
    }
    const granted = varta.json.parse<Record<string, string>>(text);
    for (const [name, value] of Object.entries(granted)) {
      env[name] = value;
    }
    return env;
  }
}

// `typeof globalThis`, restricted to its typed members. Each read builds a
// fresh member object (Varta's globals hold no state of their own).
export class GlobalObject {
  get globalThis(): GlobalObject {
    return new GlobalObject();
  }

  get crypto(): Crypto {
    return new Crypto();
  }

  get process(): Process {
    return new Process();
  }
}

export function globalObject(): GlobalObject {
  return new GlobalObject();
}

// `process.env.NAME` (a compiler hook): one `env.get` of `name`, checked
// before the read; a denied or unset variable is `undefined`.
export function environmentValue(name: string): string | undefined {
  const text = name === '' ? '' : varta.env.snapshot(name);
  if (text === '') {
    return undefined;
  }
  const found = varta.json.parse<Record<string, string>>(text);
  return found[name];
}
