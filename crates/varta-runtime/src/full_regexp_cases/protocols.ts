// The RegExp symbol protocols (ECMAScript 2025 §22.1.3, §22.2.6.8–14):
// custom matchers called by the String methods, and RegExp's own
// [Symbol.match], [Symbol.matchAll], [Symbol.replace], [Symbol.search] and
// [Symbol.split].
class Vowels {
  calls = 0;
  [Symbol.match](text: string): string[] {
    this.calls++;
    return text.split('').filter((ch) => 'aeiou'.includes(ch));
  }
  [Symbol.replace](text: string, replacement: string): string {
    this.calls++;
    return text.replace(/[aeiou]/g, replacement);
  }
  [Symbol.search](text: string): number {
    return text.search(/[aeiou]/);
  }
  [Symbol.split](text: string, limit?: number): string[] {
    return text.split(/[aeiou]/, limit);
  }
}

// A matchAll-only matcher (no [Symbol.match], so no IsRegExp flags check).
class Positions {
  [Symbol.matchAll](text: string): Generator<number, void, undefined> {
    return this.positions(text);
  }
  *positions(text: string): Generator<number, void, undefined> {
    for (let index = 0; index < text.length; index++) {
      if ('aeiou'.includes(text[index] ?? '')) {
        yield index;
      }
    }
  }
}

export function main(): void {
  const vowels = new Vowels();
  console.log('banana'.match(vowels).join(','));
  console.log('banana'.replace(vowels, '_'));
  console.log(String('xyzabc'.search(vowels)) + ' ' + 'banana'.split(vowels).join('|') + ' ' + 'banana'.split(vowels, 2).join('|'));
  console.log([...'radio'.matchAll(new Positions())].join(',') + ' ' + String(vowels.calls));
  // RegExp's own protocol methods.
  const re = /(\d)(x)?/g;
  console.log(re[Symbol.replace]('a1b2', '<$1>') + ' ' + String(re[Symbol.search]('ab3')));
  const found = re[Symbol.match]('a1b2');
  console.log(found === null ? 'none' : found.join('+'));
  console.log(re[Symbol.split]('a1b2', 3).map((part) => part ?? '~').join('|'));
  const once = /\d/;
  console.log([...once[Symbol.matchAll]('a1b2')].map((m) => m[0]).join(','));
  console.log([...re[Symbol.matchAll]('a1b2')].map((m) => m.index).join(','));
}
