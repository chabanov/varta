// A typical request's header work: a record init, appends, reads, a set and a
// delete, one for-of iteration and one forEach (v35 `web_headers`). The test
// pins its production fuel and host-call cost as well as its output.
function retryAfter(headers: Headers): number | undefined {
  const value = headers.get('Retry-After');
  if (value === null) return undefined;
  const parsed = Number(value);
  return Number.isNaN(parsed) ? undefined : parsed;
}

export function main(): void {
  const headers = new Headers({
    'Content-Type': 'application/json; charset=utf-8',
    Authorization: 'Bearer token-123',
    'User-Agent': 'uarp-sdk/0.12.0',
    Accept: 'application/json',
  });
  headers.append('X-Request-Id', 'req-42');
  headers.append('Retry-After', ' 30 ');
  headers.append('Set-Cookie', 'a=1');
  headers.append('Set-Cookie', 'b=2');
  headers.set('Cache-Control', 'no-store');
  headers.append('Content-Length', '120');
  headers.append('Accept-Encoding', 'gzip');
  headers.delete('accept-encoding');
  console.log(`${retryAfter(headers)} ${headers.has('authorization')} ${headers.has('cookie')}`);
  let names = '';
  for (const [name] of headers) {
    // Without tuple types the destructured name is `string | undefined`.
    names = names === '' ? `${name}` : `${names},${name}`;
  }
  console.log(names);
  let count = 0;
  headers.forEach(() => {
    count++;
  });
  console.log(`${count}`);
}
