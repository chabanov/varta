// Engine prelude of checked-f64-utf16-v38 (`symbol_values`, K9 D2):
// ECMAScript `Symbol` values (ECMA-262 §20.4) in typed guest code, with no
// host import and no capability. The compiler loads it when a program names
// `Symbol`, lays every `symbol` value out as the private `SymbolValue`
// record below (its block address is the symbol's identity; the value18
// symbol tag marks it in every cell, slot and element) and calls these
// functions for `Symbol()`, `Symbol.for`, `Symbol.keyFor`, the well-known
// symbols, `description` and `toString`; see docs/architecture.md
// (Symbol values).
//
// The global symbol registry (§20.4.2.2, GlobalSymbolRegistry) is a
// dictionary from key to symbol in guest memory: it belongs to the
// program's instance like every module constant, so each instance (each
// HTTP request Store) starts with an empty registry.

export class SymbolValue {
  // The description, or '' for a symbol without one.
  description: string;
  // Whether the symbol has a description (`Symbol('')` has the empty one).
  described: boolean;
  constructor(description: string, described: boolean) {
    this.description = description;
    this.described = described;
  }
}

const REGISTRY: { [key: string]: SymbolValue } = {};

// The well-known symbols in `wellKnownSymbol` index order (§6.1.5.1), each
// created on first use; their descriptions are `Symbol.<name>`.
const WELL_KNOWN: SymbolValue[] = [];
const WELL_KNOWN_NAMES = [
  'Symbol.asyncIterator',
  'Symbol.hasInstance',
  'Symbol.isConcatSpreadable',
  'Symbol.iterator',
  'Symbol.match',
  'Symbol.matchAll',
  'Symbol.replace',
  'Symbol.search',
  'Symbol.species',
  'Symbol.split',
  'Symbol.toPrimitive',
  'Symbol.toStringTag',
  'Symbol.unscopables',
  'Symbol.asyncDispose',
  'Symbol.dispose',
];

// `Symbol()` and `Symbol(undefined)`.
export function symbolCreate(): SymbolValue {
  return new SymbolValue('', false);
}

// `Symbol(description)` for a string description.
export function symbolDescribed(description: string): SymbolValue {
  return new SymbolValue(description, true);
}

// `Symbol(description)` for an optional string description.
export function symbolOf(description: string | undefined): SymbolValue {
  if (description === undefined) {
    return new SymbolValue('', false);
  }
  return new SymbolValue(description, true);
}

// `Symbol.for(key)` (§20.4.2.2).
export function symbolFor(key: string): SymbolValue {
  const found = REGISTRY[key];
  if (found !== undefined) {
    return found;
  }
  const created = new SymbolValue(key, true);
  REGISTRY[key] = created;
  return created;
}

// `Symbol.keyFor(symbol)` (§20.4.2.6): a registered symbol's key is its
// description.
export function symbolKeyFor(symbol: SymbolValue): string | undefined {
  if (symbol.described && REGISTRY[symbol.description] === symbol) {
    return symbol.description;
  }
  return undefined;
}

// The well-known symbol at `index` of `WELL_KNOWN_NAMES`.
export function wellKnownSymbol(index: number): SymbolValue {
  if (WELL_KNOWN.length === 0) {
    for (const name of WELL_KNOWN_NAMES) {
      WELL_KNOWN.push(new SymbolValue(name, true));
    }
  }
  return WELL_KNOWN[index];
}

// `symbol.description` (§20.4.3.2).
export function symbolDescription(symbol: SymbolValue): string | undefined {
  return symbol.described ? symbol.description : undefined;
}

// `symbol.toString()` and `String(symbol)` (§20.4.3.3.1
// SymbolDescriptiveString).
export function symbolToString(symbol: SymbolValue): string {
  return 'Symbol(' + symbol.description + ')';
}
