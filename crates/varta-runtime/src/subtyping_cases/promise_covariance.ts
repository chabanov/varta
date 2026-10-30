type PageFetcher = (cursor: string | undefined) => Promise<unknown>;
interface Page {
  items: string[];
  cursor: string | null;
}
async function fetchPage(cursor: string | undefined): Promise<Page> {
  await null;
  return cursor === undefined ? { items: ["a", "b"], cursor: "c1" } : { items: ["c"], cursor: null };
}
async function walk(fetch: PageFetcher): Promise<string> {
  let cursor: string | undefined;
  let out = "";
  for (let i = 0; i < 3; i++) {
    const page = (await fetch(cursor)) as Record<string, unknown> | undefined;
    if (!page) return out;
    const items = page["items"];
    if (Array.isArray(items)) {
      for (const item of items) out += `${String(item)},`;
    }
    const next = page["cursor"];
    if (typeof next !== "string") return out;
    cursor = next;
  }
  return out;
}
export async function main(): Promise<void> {
  console.log(await walk((cursor) => fetchPage(cursor)));
}
