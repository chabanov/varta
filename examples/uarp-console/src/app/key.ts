// The UARP API key, kept in the console's KV database (`varta.kv`, Deno KV
// file format). For now nothing in the console writes it; the settings screen
// will, and the tests seed it with `writeApiKey`.

/** The key's entry in the database. */
export const API_KEY_ENTRY: KvKey = ["uarp", "apiKey"];

/** The stored key, or null when there is none. */
export async function readApiKey(kv: Kv): Promise<string | null> {
  const entry = await kv.get<string>(API_KEY_ENTRY);
  return entry.value;
}

export async function writeApiKey(kv: Kv, key: string): Promise<void> {
  await kv.set<string>(API_KEY_ENTRY, key);
}
