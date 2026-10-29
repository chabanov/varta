// The Headers example of docs/language-guide.md (v35).
function numericHeader(headers: Headers, name: string): number | undefined {
  const value = headers.get(name);
  if (value === null) return undefined;
  const parsed = Number(value);
  return Number.isNaN(parsed) ? undefined : parsed;
}

export function main(): void {
  const headers = new Headers({ 'Content-Type': 'application/json' });
  headers.append('Retry-After', ' 30 ');           // value whitespace is trimmed
  headers.append('accept', 'text/plain');
  headers.append('Accept', 'application/json');
  console.log(headers.get('ACCEPT'));               // text/plain, application/json
  console.log(String(numericHeader(headers, 'retry-after'))); // 30
  for (const [name, value] of headers) {
    console.log(`${name}: ${value}`);               // accept, content-type, retry-after
  }
  headers.forEach((value, name) => {
    if (name === 'content-type') console.log(value); // application/json
  });
  try {
    headers.set('bad name', 'x');
  } catch (error) {
    if (error instanceof TypeError) console.log(error.message);
    // Headers.set: "bad name" is an invalid header name.
  }
}
