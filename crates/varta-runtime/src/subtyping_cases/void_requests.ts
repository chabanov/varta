async function decode(text: string): Promise<unknown> {
  await null;
  return text === '' ? undefined : JSON.parse(text);
}
class Client {
  calls = 0;
  async request<T>(text: string): Promise<T> {
    this.calls += 1;
    return (await decode(text)) as T;
  }
}
export async function main(): Promise<void> {
  const client = new Client();
  await client.request<void>('');
  const n = await client.request<number>('4');
  console.log(`${n} ${client.calls}`);
}
