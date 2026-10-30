interface Settings { cancel?: AbortSignal; label?: string }

function delay(ms: number, settings: Settings): Promise<string> {
  return new Promise((resolve, reject) => {
    // The timer callback names `stop`, declared below: a capture before
    // initialization, legal because the callback runs later.
    const handle = setTimeout(() => {
      settings.cancel?.removeEventListener("abort", stop);
      resolve(describe());
    }, ms);
    const stop = () => {
      clearTimeout(handle);
      reject(settings.cancel?.reason);
    };
    settings.cancel?.addEventListener("abort", stop);
    // Nested function declarations are hoisted to the block entry.
    function describe(): string {
      // Property-path narrowing: `settings.label` is a string here.
      return settings.label !== undefined ? settings.label : "done";
    }
  });
}

function kind(field: string): string {
  let out: string;                 // assigned on every path before the read
  switch (field) {                 // strict equality over strings
    case "id":
    case "key":
      out = "key";
      break;
    default:
      out = "value";
  }
  return out;
}

export async function main(): Promise<void> {
  console.log(await delay(5, { label: "slept" }));      // slept
  const parts: string[] = ["id", "x"];
  let joined = "";
  for (const part of parts) joined += kind(part);        // part: string
  console.log(joined + " " + parts[0]);                  // keyvalue id
  const seen: number[] = [];
  const record = (n: number): number => seen.push(n);   // returns a number
  [1, 2].forEach(record);                                // as a void callback
  const read = (): number => late;
  try {
    read();
  } catch (e) {
    if (e instanceof Error) console.log(e.name);         // ReferenceError
  }
  const late = 1;
  console.log(String(read()));                           // 1
}
