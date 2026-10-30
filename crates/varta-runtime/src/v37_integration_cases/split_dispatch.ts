// One dispatch path for `split` (collections and RegExp): string and RegExp
// separators, limits, and own methods or function fields named `split`.
class Splitter {
  split(text: string): string[] {
    return text.split(',');
  }
}
interface Tokenizer {
  split: (text: string) => string[];
}
export function main(): void {
  console.log(
    JSON.stringify([
      'a,b,,c'.split(','),
      'a1b22c'.split(/\d+/),
      'abc'.split(''),
      'a b'.split(),
      'a,b,c'.split(',', 2),
      'x1y2z'.split(/(\d)/),
      ''.split(','),
      ''.split(/,/),
      'a,b'.split(/,/, 1),
    ]),
  );
  console.log(new Splitter().split('p,q').join('|'));
  const tokenizer: Tokenizer = { split: (text: string): string[] => [text, text] };
  console.log(String(tokenizer.split('z').length));
  const pattern: RegExp = /-/;
  const separator: string = '-';
  console.log(`${'a-b'.split(pattern).length} ${'a-b-c'.split(separator, 2).join('+')}`);
}
