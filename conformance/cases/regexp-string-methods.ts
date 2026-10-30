export function main(): void {
  console.log('John Smith'.replace(/(\w+)\s(\w+)/, '$2, $1') + ' ' + 'a-b-c'.replaceAll(/-/g, '+'));
  console.log(JSON.stringify('a1b22c'.split(/(\d)+/)) + ' ' + JSON.stringify('a,b'.split(',')));
  let found = '';
  for (const m of 'x1y22'.matchAll(/\d+/g)) {
    found = found + m![0] + '@' + `${m!.index};`;
  }
  console.log(found + ' ' + `${'line\r\nnext'.search(/\r\n|\r|\n/)}`);
  console.log('one two'.replace(/\w+/g, (word) => `${word.length}`));
}
