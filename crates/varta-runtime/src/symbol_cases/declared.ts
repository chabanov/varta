// Declared symbols: `unique symbol` constants, symbol class fields,
// parameters, results, optional symbols and closures over symbols.
const RED: unique symbol = Symbol('red');
const BLUE = Symbol('blue');

class Token {
  readonly kind: symbol;
  label?: symbol;
  constructor(kind: symbol) {
    this.kind = kind;
  }
  is(kind: symbol): boolean {
    return this.kind === kind;
  }
}

function pick(flag: boolean): symbol {
  return flag ? RED : BLUE;
}

function name(kind: symbol | undefined): string {
  if (kind === undefined) {
    return 'none';
  }
  if (kind === RED) {
    return 'red';
  }
  return kind.description ?? '?';
}

export function main(): void {
  const token = new Token(RED);
  console.log(String(token.is(RED)));
  console.log(String(token.is(BLUE)));
  console.log(name(token.label));
  token.label = BLUE;
  console.log(name(token.label));
  console.log(name(pick(true)));
  console.log(name(pick(false)));
  const remember = (): symbol => token.kind;
  console.log(String(remember() === RED));
  console.log(JSON.stringify(token));
  let current: symbol = RED;
  current = Symbol.for('red');
  console.log(String(current === RED));
  console.log(String(Symbol.keyFor(current)));
}
