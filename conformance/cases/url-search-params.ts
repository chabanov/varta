// WHATWG URL parsing and form-urlencoded query lists (Node.js v24.20.0
// output). Under the v36 pin URL is an unknown class; the v37 `net_request`
// prelude implements this subset in guest code.
export function main(): void {
  const url = new URL('../x/y?z=1', 'HTTP://Example.TEST:80/p/q/r');
  console.log(url.href);
  const params = new URLSearchParams('a=1&b=x+y&a=2');
  params.append('c', 'd e');
  console.log(`${params.getAll('a').length} ${params.get('b')} ${params.toString()}`);
}
