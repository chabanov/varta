// V38 KV without a grant: opening a database rejects with a recoverable
// NotAllowedError DOMException before any file is touched, and the program
// continues. KvU64 values are checked unsigned 64-bit wrappers.
interface Counter {
  label: string;
  hits: number;
}

async function openCounter(path: string): Promise<string> {
  try {
    const kv = await varta.kv.open(path);
    const entry = await kv.get<Counter>(['counter', 1]);
    kv.close();
    return entry.value === null ? 'empty' : entry.value.label;
  } catch (error) {
    return error instanceof DOMException ? `${error.name}` : 'other';
  }
}

export async function main(): Promise<void> {
  console.log(await openCounter('/data/app.kv'));
  const big = new KvU64(18446744073709551615n);
  console.log(`${big.value} ${big.toString()} ${new KvU64(0n).value === 0n}`);
  try {
    new KvU64(-1n);
  } catch (error) {
    console.log(`negative ${error instanceof RangeError}`);
  }
  console.log('continued');
}
