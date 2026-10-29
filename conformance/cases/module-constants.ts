const SDK_VERSION = "0.12.0";
const USER_AGENT = "uarp-sdk/" + SDK_VERSION;
const SCOPES = ["admin", "agents:read"] as const;
const LIMITS = { page: 50, retries: 2 };
const SEEN: string[] = [];
const WIDE = 18446744073709551616n;
const MISSING = undefined;

function remember(value: string): number {
  return SEEN.push(value);
}

function scopeAt(index: number): string {
  const scope = SCOPES[index];
  return scope === undefined ? "none" : scope;
}

export function main(): void {
  console.log(USER_AGENT);
  console.log(scopeAt(1) + " " + scopeAt(2) + " " + String(SCOPES.length));
  remember("a");
  console.log(String(remember("b")) + " " + String(SEEN.length));
  LIMITS.page = LIMITS.page + 1;
  console.log(JSON.stringify(LIMITS));
  const version = (): string => SDK_VERSION;
  console.log(version());
  console.log(String(WIDE + 1n));
  console.log(MISSING === undefined);
}
