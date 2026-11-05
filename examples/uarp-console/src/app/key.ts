// The console's settings, kept in its KV database (`varta.kv`, Deno KV file
// format): the UARP API key and the API base URL. The settings screen writes
// them through `SettingsStore` and nothing else holds the key: it never enters
// a signal, the view, a log line, the artifact or the manifest; the console
// reads it at start and gives it only to the client it constructs.

/** The key's entry in the database. */
export const API_KEY_ENTRY: KvKey = ["uarp", "apiKey"];
/** The base URL's entry; absent means the product's default (config.ts). */
export const BASE_URL_ENTRY: KvKey = ["uarp", "baseURL"];

/** The stored settings; null where nothing is stored. */
export interface Settings {
  baseURL: string | null;
  apiKey: string | null;
}

/** Where the settings live. The product's store is the KV database; view tests hold them in memory. */
export interface SettingsStore {
  read(): Promise<Settings>;
  /** Stores the base URL and the key, replacing what was there. */
  write(baseURL: string, apiKey: string): Promise<void>;
  /** Removes the stored key; the base URL stays. */
  forget(): Promise<void>;
}

/** The stored key, or null when there is none. */
export async function readApiKey(kv: Kv): Promise<string | null> {
  const entry = await kv.get<string>(API_KEY_ENTRY);
  return entry.value;
}

export async function writeApiKey(kv: Kv, key: string): Promise<void> {
  await kv.set<string>(API_KEY_ENTRY, key);
}

/** The settings in a KV database; every operation opens it and closes it again, also on failure. */
export class KvSettings implements SettingsStore {
  database: string;

  constructor(database: string) {
    this.database = database;
  }

  async read(): Promise<Settings> {
    const kv = await varta.kv.open(this.database);
    try {
      const key = await readApiKey(kv);
      const base = await kv.get<string>(BASE_URL_ENTRY);
      return { baseURL: base.value, apiKey: key };
    } finally {
      kv.close();
    }
  }

  async write(baseURL: string, apiKey: string): Promise<void> {
    const kv = await varta.kv.open(this.database);
    try {
      await kv.set<string>(BASE_URL_ENTRY, baseURL);
      await writeApiKey(kv, apiKey);
    } finally {
      kv.close();
    }
  }

  async forget(): Promise<void> {
    const kv = await varta.kv.open(this.database);
    try {
      await kv.delete(API_KEY_ENTRY);
    } finally {
      kv.close();
    }
  }
}
