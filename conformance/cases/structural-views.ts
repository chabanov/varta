// V38 structural subtyping: object literals and class instances viewed as
// narrower interfaces, writes through a view, callbacks with fewer
// parameters than their context, a function with an extra optional
// parameter passed where none is expected, and a boolean-field truth guard.
interface Labeled {
  label: string;
}
interface Sized extends Labeled {
  readonly size: number;
}
interface Page<T> {
  items: T[];
  next?: string;
}

class Folder {
  label: string;
  #children: string[] = [];
  constructor(label: string) {
    this.label = label;
  }
  get size(): number {
    return this.#children.length;
  }
  add(name: string): Folder {
    this.#children.push(name);
    return this;
  }
}

function title(item: Labeled): string {
  return item.label;
}
function describe(item: Sized): string {
  return `${item.label}:${item.size}`;
}
function relabel(item: Labeled, label: string): void {
  item.label = label;
}
function apply(values: number[], visit: (value: number, index: number, all: number[]) => string): string {
  return values.map((value, index) => visit(value, index, values)).join(',');
}
function call(run: (name: string) => string): string {
  return run('x');
}
function greet(name: string, punctuation?: string): string {
  return `hi ${name}${punctuation ?? '!'}`;
}

type Result = { ok: true; value: number } | { ok: false; error: string };
function settle(result: Result): string {
  return result.ok ? `value ${result.value}` : `error ${result.error}`;
}

export function main(): void {
  const literal = { label: 'lit', size: 2, extra: true };
  const folder = new Folder('docs').add('a').add('b').add('c');
  console.log(`${title(literal)} ${title(folder)} ${describe(literal)} ${describe(folder)}`);
  const views: Labeled[] = [literal, folder, { label: 'inline' }];
  console.log(views.map(title).join(' '));
  relabel(literal, 'renamed');
  relabel(folder, 'moved');
  console.log(`${literal.label} ${folder.label} ${literal.extra}`);
  console.log(apply([4, 5, 6], (value) => `${value * 2}`));
  console.log(apply([4, 5], (value, index) => `${index}=${value}`));
  console.log(`${call(greet)} ${greet('y', '?')}`);
  const page: Page<Labeled> = { items: [folder, literal] };
  console.log(`${page.items.length} ${page.next ?? 'end'} ${page.items[0] === folder}`);
  console.log(`${settle({ ok: true, value: 3 })} ${settle({ ok: false, error: 'no' })}`);
}
