// Fetch Headers: case-insensitive names, combined values, HTTP whitespace
// normalization, set-cookie entries, sorted iteration and TypeError validation.
function dump(headers: Headers): string {
  let out = '';
  for (const [name, value] of headers) {
    out = out + `[${name}=${value}]`;
  }
  return out;
}

export function main(): void {
  const headers = new Headers({ 'Content-Type': 'text/plain', Accept: '*/*' });
  headers.append('accept', ' application/json\t');
  headers.append('Set-Cookie', 'a=1');
  headers.append('set-cookie', 'b=2');
  headers.set('X-Id', '7');
  headers.delete('content-type');
  console.log(`${headers.get('ACCEPT')} ${headers.has('x-id')} ${headers.get('missing')}`);
  console.log(dump(headers));
  console.log(JSON.stringify(headers.getSetCookie()));
  try {
    headers.append('bad name', 'x');
  } catch (error) {
    if (error instanceof TypeError) console.log(error.message);
  }
  const copy = new Headers(headers);
  copy.set('x-id', '8');
  console.log(`${headers.get('x-id')} ${copy.get('x-id')} ${copy instanceof Headers}`);
}
