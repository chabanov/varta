// Rejections reach `catch (e)` as `unknown` across awaits; `instanceof`
// and checked `as` recover Error subclasses, their `unknown` causes and
// dictionaries held by `unknown`.
class ApiError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "ApiError";
  }
}
interface Detail {
  code: number;
}
async function request(kind: number): Promise<number> {
  await null;
  if (kind === 0) throw new ApiError("status", { cause: { code: 503 } });
  if (kind === 1) throw new RangeError("range");
  if (kind === 2) {
    const headers: Record<string, string> = { retry: "later" };
    throw new ApiError("headers", { cause: headers });
  }
  if (kind === 3) throw "text";
  return kind;
}
async function attempt(kind: number): Promise<string> {
  try {
    const value = await request(kind);
    return `ok ${value}`;
  } catch (e) {
    if (e instanceof ApiError) {
      const cause = e.cause;
      if (kind === 0) return `${e.name} ${(cause as Detail).code}`;
      const headers = cause as Record<string, string>;
      return `${e.message} ${headers["retry"]} ${String(cause)}`;
    }
    if (e instanceof Error) return `${e.name}: ${e.message}`;
    if (typeof e === "string") return `string ${e}`;
    return "other";
  }
}
export async function main(): Promise<void> {
  for (let kind = 0; kind < 5; kind++) {
    console.log(await attempt(kind));
  }
  const held: unknown = { a: "x" } as Record<string, string>;
  await null;
  const back = held as Record<string, string>;
  back["b"] = "y";
  console.log(JSON.stringify(Object.entries(back)));
}
