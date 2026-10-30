// V37 streamed bodies: server-sent events read with `for await` (the body's
// `[Symbol.asyncIterator]`), and a reader over UTF-8 split between chunks.
// Varta chunks are decoded text; the Node runner pipes Node's byte chunks
// through TextDecoderStream so both print the same.
const BASE = '__BASE__';

async function events(): Promise<void> {
  const response = await fetch(BASE + '/sse');
  const body = response.body;
  if (body === null) {
    return;
  }
  let text = '';
  for await (const chunk of body) {
    text += chunk;
  }
  console.log(`${response.headers.get('content-type')} ${text.length} ${text.indexOf('tick 2')} ${text.endsWith('data: tick 3\n\n')}`);
}

async function split(): Promise<void> {
  const response = await fetch(BASE + '/utf8');
  const body = response.body;
  if (body === null) {
    return;
  }
  const reader = body.getReader();
  let text = '';
  for (;;) {
    const { done, value } = await reader.read();
    if (done) {
      break;
    }
    text += value;
  }
  console.log(`${text} ${text.length} ${body.locked}`);
  reader.releaseLock();
  console.log(`${body.locked}`);
}

export async function main(): Promise<void> {
  await events();
  await split();
}
