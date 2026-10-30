// for...of over strings visits code points; a switch whose clauses cover a
// literal union needs no default to return on every path.
type Kind = 'a' | 'b' | 'c';
type Level = 1 | 2;
function label(k: Kind): string {
  switch (k) {
    case 'a':
      return 'A';
    case 'b':
      return 'B';
    case 'c':
      return 'C';
  }
}
function weight(level: Level): number {
  switch (level) {
    case 1:
      return 10;
    case 2:
      return 20;
  }
}
function letters(text: string): string {
  let out = '';
  let count = 0;
  for (const ch of text) {
    count += 1;
    if (ch === ' ') continue;
    out += `[${ch}]`;
  }
  return `${out} ${count}`;
}
export function main(): void {
  console.log(letters('héllo 😀!'));
  let last = '';
  for (last of 'xyz') {
    // assignment head
  }
  console.log(`${last} ${label('b')}${label('c')} ${weight(2)}`);
}
