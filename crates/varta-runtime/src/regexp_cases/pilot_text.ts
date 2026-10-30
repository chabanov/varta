// The shapes of the pilot's regex use: URL joining (`replace` with anchored
// patterns, a case-insensitive scheme test), query component escaping (a
// class test per character) and SSE line splitting (`search` and `exec`
// over a growing buffer).
function joinUrl(baseUrl: string, path: string): string {
  if (/^https?:\/\//i.test(path)) {
    return path;
  }
  return baseUrl.replace(/\/+$/, '') + '/' + path.replace(/^\/+/, '');
}
function encode(value: string): string {
  let out = '';
  for (let i = 0; i < value.length; i++) {
    const char = value.slice(i, i + 1);
    if (/[A-Za-z0-9\-._~]/.test(char)) {
      out += char;
    } else {
      out += char === ' ' ? '%20' : '%2B';
    }
  }
  return out;
}
function lines(buffer: string): string[] {
  const found: string[] = [];
  // The pilot declares `let newline: number;` and assigns it in the loop
  // condition; deferred initialization belongs to the language-core stream.
  let newline = buffer.search(/\r\n|\r|\n/);
  for (; newline !== -1; newline = buffer.search(/\r\n|\r|\n/)) {
    const matched = /\r\n|\r|\n/.exec(buffer.slice(newline))![0];
    if (matched === '\r' && newline === buffer.length - 1) {
      break;
    }
    found.push(buffer.slice(0, newline));
    buffer = buffer.slice(newline + matched.length);
  }
  return found;
}
export function main(): void {
  console.log(joinUrl('https://api.example.com/', '/v1/models'));
  console.log(joinUrl('https://api.example.com/', 'HTTPS://other.example/x'));
  console.log(encode('a b+c~'));
  const events = lines('event:message\r\ndata:{"a":1}\n\ndata:2\r\r:keep-alive\n\r');
  let text = '';
  for (let i = 0; i < events.length; i++) {
    text = text + events[i]! + '|';
  }
  console.log(text);
  console.log(`${events.length}`);
}
