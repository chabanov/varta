// V37 `fetch` basics against the loopback server: status, status text,
// headers, JSON and text bodies, a POST with headers and a JSON body.
const BASE = '__BASE__';

async function json(): Promise<void> {
  const response = await fetch(BASE + '/json');
  console.log(`${response.status} ${response.statusText} ${response.ok} ${response.headers.get('content-type')}`);
  const value = (await response.json()) as { ok: boolean; items: number[] };
  console.log(`${value.ok} ${value.items.length} ${response.bodyUsed}`);
}

async function post(): Promise<void> {
  const headers = new Headers();
  headers.set('Authorization', 'Bearer k');
  headers.set('Content-Type', 'application/json');
  const response = await fetch(BASE + '/echo?x=1', {
    method: 'post',
    headers: headers,
    body: JSON.stringify({ a: 1 }),
  });
  console.log(await response.text());
}

async function missing(): Promise<void> {
  const response = await fetch(new Request(BASE + '/missing'));
  console.log(`${response.status} ${response.ok} ${response.statusText} ${await response.text()}`);
  try {
    await response.text();
  } catch (error) {
    console.log(`reread: ${error instanceof TypeError}`);
  }
}

async function form(): Promise<void> {
  const url = new URL('/echo', BASE);
  url.searchParams.append('q', 'a b');
  const response = await fetch(url.href, { method: 'PUT', body: new URLSearchParams({ x: '1 2' }) });
  console.log(await response.text());
}

export async function main(): Promise<void> {
  await json();
  await post();
  await form();
  await missing();
  console.log('done');
}
