// V37 redirects: `error` (the Varta default) refuses them, `manual` returns
// the 3xx response, `follow` issues a new request that the grant checks again.
const BASE = '__BASE__';

export async function main(): Promise<void> {
  try {
    await fetch(BASE + '/redirect', { redirect: 'error' });
  } catch (error) {
    console.log(`error: ${error instanceof TypeError} ${error instanceof Error ? error.message : ''}`);
  }
  const manual = await fetch(BASE + '/redirect', { redirect: 'manual' });
  console.log(`manual: ${manual.status} ${manual.headers.get('location')} ${manual.redirected}`);
  const followed = await fetch(BASE + '/redirect', { redirect: 'follow' });
  console.log(`follow: ${followed.status} ${followed.redirected} ${followed.url === BASE + '/hello'} ${await followed.text()}`);
}
