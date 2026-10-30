// V37 URL and URLSearchParams (a WHATWG subset in guest code): parsing,
// normalization, relative references, percent-encoding and query lists.
export function main(): void {
  const url = new URL('HTTPS://Example.COM:443/a/./b/../c d?q=1 2#frag ment');
  console.log(url.href);
  console.log(`${url.protocol} ${url.host} ${url.hostname} ${url.port === ''} ${url.pathname} ${url.search} ${url.hash} ${url.origin}`);
  const relative = new URL('../x/y?z=é', 'http://h.test:8080/p/q/r');
  console.log(relative.href);
  console.log(new URL('//other.test/k', relative).href);
  console.log(new URL('?only', 'http://h.test/p/q').href);
  console.log(new URL('#f', 'http://h.test/p?q').href);
  console.log(`${URL.canParse('http://ok.test')} ${URL.canParse('/rel')} ${URL.canParse('/rel', 'http://a.test')}`);
  try {
    new URL('http://bad host/');
  } catch (error) {
    console.log(`${error instanceof TypeError} ${error instanceof Error ? error.message : ''}`);
  }
  const params = new URLSearchParams('a=1&b=x+y&a=%7E%41&empty');
  console.log(`${params.size} ${params.get('a')} ${params.getAll('a').length} ${params.get('b')} ${params.has('empty')} ${params.get('none')}`);
  params.append('c', 'é & ü');
  params.set('a', '3');
  params.delete('empty');
  console.log(params.toString());
  const linked = new URL('http://h.test/path?x=1');
  linked.searchParams.append('y', 'two words');
  console.log(linked.href);
  console.log(`${linked.searchParams.get('y')} ${linked.search}`);
  const record = new URLSearchParams({ name: 'Ada Lovelace', tag: '*-._~' });
  console.log(record.toString());
}
