export function pick<T extends object, K extends readonly (keyof T & string)[]>(
  source: T | undefined,
  keys: K,
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (!source) return out;
  for (const key of keys) {
    const value = (source as Record<string, unknown>)[key];
    if (value !== undefined) out[key] = value;
  }
  return out;
}
interface Params {
  subject_id: string;
  limit?: number;
  other?: string;
}
export function main(): void {
  const p: Params = { subject_id: "s1", limit: 3 };
  console.log(JSON.stringify(pick(p, ["subject_id", "limit"])));
  console.log(JSON.stringify(pick(undefined as Params | undefined, ["subject_id"])));
}
