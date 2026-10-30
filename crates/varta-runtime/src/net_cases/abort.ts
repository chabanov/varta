// V37 cancellation: an AbortController aborts a request in flight, an
// already aborted signal rejects at once, AbortSignal.timeout ends a request
// that never answers, and aborting a streamed body rejects the pending read.
const BASE = '__BASE__';

function describe(error: unknown): string {
  if (error instanceof DOMException) {
    return `${error.name}: ${error.message}`;
  }
  return error instanceof Error ? `${error.name}: ${error.message}` : 'other';
}

export async function main(): Promise<void> {
  const controller = new AbortController();
  const pending = fetch(BASE + '/slow', { signal: controller.signal });
  setTimeout(() => controller.abort(), 20);
  try {
    await pending;
  } catch (error) {
    console.log(`aborted: ${describe(error)}`);
  }
  try {
    await fetch(BASE + '/hello', { signal: AbortSignal.abort('stop') });
  } catch (error) {
    console.log(`pre-aborted: ${error === 'stop'}`);
  }
  try {
    await fetch(BASE + '/slow', { signal: AbortSignal.timeout(30) });
  } catch (error) {
    console.log(`timeout: ${describe(error)}`);
  }
  const body = new AbortController();
  const response = await fetch(BASE + '/sse', { signal: body.signal });
  const stream = response.body;
  if (stream !== null) {
    const reader = stream.getReader();
    const first = await reader.read();
    console.log(`first: ${first.done} ${first.value.length > 0}`);
    body.abort(new Error('enough'));
    try {
      await reader.read();
    } catch (error) {
      console.log(`body: ${describe(error)}`);
    }
  }
  console.log('done');
}
