// V38 strict members over a wide discriminated union: twenty record
// interfaces selected by their `type` literal from JSON, an exhaustive
// `switch` that ends in `never`, declared optional fields read through `?.`
// and `??`, and a literal computed key of a declared field.
interface Meta {
  source?: string;
  retry?: { after: number };
}
interface E01 { type: 'e01'; n: number; meta?: Meta }
interface E02 { type: 'e02'; n: number; meta?: Meta }
interface E03 { type: 'e03'; text: string }
interface E04 { type: 'e04'; text: string }
interface E05 { type: 'e05'; flag: boolean }
interface E06 { type: 'e06'; flag: boolean }
interface E07 { type: 'e07'; items: string[] }
interface E08 { type: 'e08'; items: string[] }
interface E09 { type: 'e09'; n: number; meta?: Meta }
interface E10 { type: 'e10'; text: string }
interface E11 { type: 'e11'; flag: boolean }
interface E12 { type: 'e12'; items: string[] }
interface E13 { type: 'e13'; n: number; meta?: Meta }
interface E14 { type: 'e14'; text: string }
interface E15 { type: 'e15'; flag: boolean }
interface E16 { type: 'e16'; items: string[] }
interface E17 { type: 'e17'; n: number; meta?: Meta }
interface E18 { type: 'e18'; text: string }
interface E19 { type: 'e19'; flag: boolean }
interface E20 { type: 'e20'; items: string[] }
type Event =
  | E01 | E02 | E03 | E04 | E05 | E06 | E07 | E08 | E09 | E10
  | E11 | E12 | E13 | E14 | E15 | E16 | E17 | E18 | E19 | E20;

function describe(e: Event): string {
  switch (e.type) {
    case 'e01':
    case 'e02':
    case 'e09':
    case 'e13':
    case 'e17':
      return `${e.type} ${e.n} ${e.meta?.source ?? '-'} ${e.meta?.retry?.after ?? 0}`;
    case 'e03':
    case 'e04':
    case 'e10':
    case 'e14':
    case 'e18':
      return `${e.type} ${e['text'].toUpperCase()}`;
    case 'e05':
    case 'e06':
    case 'e11':
    case 'e15':
    case 'e19':
      return `${e.type} ${e.flag ? 'on' : 'off'}`;
    case 'e07':
    case 'e08':
    case 'e12':
    case 'e16':
    case 'e20':
      return `${e.type} ${e.items.length}:${e.items.join('+')}`;
    default: {
      const unreachable: never = e;
      return unreachable;
    }
  }
}

export function main(): void {
  const text =
    '[{"type":"e01","n":1},{"type":"e09","n":2,"meta":{"source":"api","retry":{"after":30}}},' +
    '{"type":"e14","text":"abc"},{"type":"e19","flag":true},{"type":"e20","items":["x","y"]},' +
    '{"type":"e17","n":-0.5,"meta":{}},{"type":"e06","flag":false},{"type":"e07","items":[]}]';
  const events = JSON.parse(text) as Event[];
  for (const event of events) {
    console.log(describe(event));
  }
  const local: E13 = { type: 'e13', n: 7, meta: { retry: { after: 5 } } };
  console.log(describe(local));
  const copy: E13 = { ...local, n: local.n + 1 };
  console.log(`${describe(copy)} ${local.n}`);
}
