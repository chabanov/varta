function parse(text: string): number;
function parse(values: string[]): number[];
function parse(input: string | string[]): number | number[] {
  if (typeof input === 'string') return Number(input);
  return input.map((item: string): number => Number(item));
}
class Formatter {
  format(value: number): string;
  format(value: boolean): string;
  format(value: number | boolean): string {
    return typeof value === 'number' ? value.toFixed(1) : value ? 'yes' : 'no';
  }
}
export function main(): void {
  const one = parse('41') + 1;
  const many = parse(['1', '2']);
  const formatter = new Formatter();
  console.log(String(one) + ' ' + String(many.length) + ' ' + formatter.format(2) + ' ' + formatter.format(false));
}
