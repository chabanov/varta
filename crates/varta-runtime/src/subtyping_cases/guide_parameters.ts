interface RequestOptions {
  timeout?: number;
}
class Runs {
  calls = 0;
  get(id: string, options?: RequestOptions): string {
    this.calls += 1;
    return `${id}#${this.calls} t=${options?.timeout ?? 30}`;
  }
}
// A narrower port over the client.
interface RunReader {
  get(id: string): string;
}
type Format = (value: number) => string;

function fixed(value: number, digits: number = 2): string {
  return value.toFixed(digits);
}

export function main(): void {
  const reader: RunReader = new Runs();
  const format: Format = fixed;
  console.log(`${reader.get('r1')} ${reader.get('r2')} ${format(1.5)}`);
  // r1#1 t=30 r2#2 t=30 1.50
}
