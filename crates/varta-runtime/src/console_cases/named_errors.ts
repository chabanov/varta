class Named extends Error {
  constructor(m: string) { super(m); this.name = 'Named'; }
}
export function main(): void {
  const n = new Named('n');
  const u: unknown = n;
  console.log(JSON.stringify(u));
  console.log(n.name);
  console.log(n);
}
