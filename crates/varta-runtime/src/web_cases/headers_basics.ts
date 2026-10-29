// Headers construction, combined values, validation messages, set-cookie,
// iteration order and the live forEach (v35 `web_headers`).
function attempt(label: string, run: () => void): void {
  try {
    run();
    console.log(`${label}: ok`);
  } catch (error) {
    if (error instanceof TypeError) {
      console.log(`${label}: ${error.name}: ${JSON.stringify(error.message)}`);
    }
  }
}

function dump(headers: Headers): string {
  let out = '';
  for (const [name, value] of headers) {
    out = out + `[${name}=${value}]`;
  }
  return out;
}

export function main(): void {
  const h = new Headers([['B', '2'], ['a', '1'], ['set-cookie', 'x=1'], ['Set-Cookie', 'y=2'], ['a', ' 3\t']]);
  console.log(dump(h));
  console.log(`${h.get('A')} ${h.get('set-cookie')} ${JSON.stringify(h.getSetCookie())}`);
  attempt('name', () => {
    new Headers({ 'a b': 'x' });
  });
  attempt('value', () => {
    new Headers({ a: 'x\ny' });
  });
  attempt('bytes', () => {
    new Headers({ a: '€' });
  });
  attempt('pair', () => {
    new Headers([['a']]);
  });
  attempt('empty', () => {
    new Headers().append('', 'x');
  });
  attempt('get', () => {
    new Headers().get('a b');
  });
  attempt('has', () => {
    new Headers().has('é');
  });
  attempt('delete', () => {
    new Headers().delete('(');
  });
  const s = new Headers();
  s.append('Set-Cookie', 'a=1');
  s.append('x', '1');
  s.append('set-cookie', 'b=2');
  s.set('X', '2');
  let keys = '';
  for (const key of s.keys()) keys = keys + `[${key}]`;
  let values = '';
  for (const value of s.values()) values = values + `[${value}]`;
  console.log(`${keys} ${values} ${JSON.stringify(s.getSetCookie())}`);
  s.set('set-cookie', 'c=3');
  console.log(dump(s));
  s.delete('SET-COOKIE');
  console.log(`${dump(s)} ${s.has('set-cookie')} ${s.get('nope')}`);
  const f = new Headers({ b: '1', a: '2', c: '3' });
  f.forEach((v, k, p) => {
    console.log(`${k} ${v} ${p === f}`);
    if (k === 'a') {
      p.delete('b');
      p.append('d', '4');
    }
  });
  f.forEach((value) => console.log(value));
  const copy = new Headers(f);
  copy.append('e', '5');
  console.log(`${dump(f)} ${dump(copy)} ${copy instanceof Headers}`);
  for (const [name, value] of copy.entries()) {
    console.log(`${name}: ${value}`);
  }
}
