interface Chunk {
  done: boolean;
  value: string;
}
function later<T>(value: T, ms: number): Promise<T> {
  return new Promise<T>((resolve) => {
    setTimeout(() => resolve(value), ms);
  });
}
async function race(ms: number): Promise<void> {
  const read: Promise<Chunk> = later({ done: false, value: 'data' }, 5);
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timeout = new Promise<'timeout'>((resolve) => {
    timer = setTimeout(() => resolve('timeout'), ms);
  });
  const result: Chunk | 'timeout' = await Promise.race([read, timeout]);
  if (timer) clearTimeout(timer);
  if (result === 'timeout') {
    console.log('timed out');
    return;
  }
  const { done, value } = result;
  console.log(`${done} ${value}`);
}
async function rejected(): Promise<void> {
  const failing = Promise.reject<number>(new Error('boom'));
  const slow = later('slow', 5);
  try {
    await Promise.race([failing, slow]);
  } catch (error) {
    console.log(error instanceof Error ? error.message : 'other');
  }
  const order: string[] = [];
  const a = Promise.resolve(1).then((v) => { order.push('a'); return v; });
  const b = Promise.resolve('b').then((v) => { order.push('b'); return v; });
  const winner = await Promise.race([a, b]);
  console.log(`${winner} ${order.join(',')}`);
}
export async function main(): Promise<void> {
  await race(1);
  await race(50);
  await rejected();
}
