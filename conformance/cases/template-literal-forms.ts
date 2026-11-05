// Template literal forms of a typed SDK: nested templates inside
// conditionals, substitutions of private fields and getters, optional
// chains with `??`, union and BigInt values, multi-line text and
// templates as Error messages and dictionary keys.
interface Options {
  userAgent?: string;
  timeout?: number;
}

class Agent {
  readonly #base: string;
  readonly #version: number;
  constructor(base: string, version: number) {
    this.#base = base;
    this.#version = version;
  }
  get name(): string {
    return `${this.#base}/${this.#version}.${this.#version * 2}`;
  }
  header(options: Options): string {
    return options.userAgent ? `${this.name} ${options.userAgent}` : this.name;
  }
}

function query(params: Record<string, string | number>): string {
  const parts: string[] = [];
  for (const key of Object.keys(params)) {
    parts.push(`${encodeURIComponent(key)}=${encodeURIComponent(`${params[key]}`)}`);
  }
  return parts.length > 0 ? `?${parts.join('&')}` : '';
}

export function main(): void {
  const agent = new Agent('sdk', 3);
  console.log(agent.header({}));
  console.log(agent.header({ userAgent: 'app/1' }));
  const options: Options | undefined = { timeout: 250 };
  console.log(`timeout=${options?.timeout ?? 60_000}ms retries=${(options as Options).userAgent ?? 'none'}`);
  console.log(`/v1/items${query({ page: 2, 'filter by': 'a&b' })}${query({})}`);
  const mixed: (string | number | boolean | null | bigint)[] = ['s', 1.5, false, null, 2n ** 64n];
  console.log(mixed.map((value) => `<${value}>`).join(''));
  const width = 3;
  console.log(`${`${width}`.padStart(width, '0')}|${'x'.repeat(width)}|${width > 2 ? `wide ${width}` : 'narrow'}`);
  const block = `line one
  line two\ttab
end`;
  console.log(`${block.split('\n').length} ${JSON.stringify(block)}`);
  const key = 'k';
  const record: Record<string, number> = {};
  record[`${key}1`] = 1;
  record[`${key}${record[`${key}1`] + 1}`] = 2;
  console.log(Object.keys(record).join(','));
  try {
    throw new RangeError(`Request timed out after ${options?.timeout} ms`);
  } catch (error) {
    console.log(`${error instanceof RangeError ? error.message : 'other'}`);
  }
}
