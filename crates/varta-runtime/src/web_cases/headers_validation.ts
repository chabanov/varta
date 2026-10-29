// WebIDL ByteString conversion, HTTP token names, value normalization and the
// order in which the constructor converts and fills (v35 `web_headers`).
function attempt(label: string, run: () => void): void {
  try {
    run();
    console.log(`${label}: ok`);
  } catch (error) {
    if (error instanceof TypeError) {
      console.log(`${label}: ${JSON.stringify(error.message)}`);
    } else {
      console.log(`${label}: unexpected`);
    }
  }
}

function dump(headers: Headers): string {
  let out = '';
  for (const [name, value] of headers) {
    out = out + `[${name}=${JSON.stringify(value)}]`;
  }
  return out;
}

export function main(): void {
  const h = new Headers();
  attempt('bytes first', () => {
    h.append('a b', '€');
  });
  attempt('name bytes', () => {
    h.append('€', 'x');
  });
  attempt('nul', () => {
    h.append('a', 'x\0');
  });
  attempt('late unit', () => {
    h.delete('aā');
  });
  attempt('set name', () => {
    h.set('a b', 'x');
  });
  attempt('set value', () => {
    h.set('a', ' \0 ');
  });
  attempt('normalized message', () => {
    new Headers({ a: ' x\0 ' });
  });
  attempt('name before value', () => {
    new Headers().append('a b', 'x\0');
  });
  attempt('pair before name', () => {
    new Headers([['a'], ['x y', 'c']]);
  });
  attempt('name before pair', () => {
    new Headers([['x y', 'b'], ['c']]);
  });
  attempt('conversion first', () => {
    new Headers([['x y', 'b'], ['€', 'c']]);
  });
  attempt('record conversion first', () => {
    new Headers({ 'x y': 'b', c: '€' });
  });
  attempt('hole pair', () => {
    new Headers([, ['a', 'b']]);
  });
  attempt('empty sequence', () => {
    new Headers([]);
  });
  const symbols = "!#$%&'*+-.^_`|~09AZaz";
  attempt('token symbols', () => {
    h.append(symbols, 'ok');
  });
  for (const bad of ['(', ')', ',', '/', ':', ';', '<', '=', '>', '?', '@', '[', '\\', ']', '{', '}', '"', ' ', '\t', 'é']) {
    try {
      h.has(bad!);
      console.log(`accepted ${JSON.stringify(bad)}`);
    } catch (error) {
      if (!(error instanceof TypeError)) console.log('unexpected');
    }
  }
  h.append('A', ' \t x \r\n');
  h.append('a', 'x\ty');
  h.append('a', '\u000b');
  h.append('a', '\u00a0x\u00a0');
  h.append('a', '');
  h.append('a', '\r\n\t ');
  console.log(JSON.stringify(h.get('a')));
  console.log(`${h.has('A')} ${h.has(symbols.slice(0, 4))} ${h.get(symbols)}`);
  console.log(dump(new Headers([['a', ,]])));
  console.log(dump(new Headers({ '2': 'two', b: 'bee', '1': 'one', A: 'upper', a: 'lower' })));
}
