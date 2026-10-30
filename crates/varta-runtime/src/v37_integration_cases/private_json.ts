// V37 JSON.stringify skips private state: engine prelude instances have no
// own properties, Error slots are non-enumerable (`name` only when a
// constructor assigns it), and #private fields are not properties.
class Point {
  x = 1;
  #secret = 2;
  get secret(): number {
    return this.#secret;
  }
}
class ApiError extends Error {
  status: number;
  constructor(message: string, status: number) {
    super(message);
    this.name = 'ApiError';
    this.status = status;
  }
}
class Plain extends Error {
  code = 7;
  constructor() {
    super('plain');
  }
}
class Aborted extends DOMException {
  reason = 'stop';
}
function describe(value: unknown): string {
  return JSON.stringify(value) ?? 'undefined';
}
export function main(): void {
  const headers = new Headers({ a: '1' });
  console.log(JSON.stringify(headers));
  console.log(JSON.stringify({ headers, n: 1 }));
  const controller = new AbortController();
  console.log(JSON.stringify([controller.signal, controller]));
  console.log(JSON.stringify(new DOMException('m', 'AbortError')));
  console.log(JSON.stringify(new Aborted('m', 'AbortError')));
  const point = new Point();
  console.log(JSON.stringify(point) + ' ' + String(point.secret));
  console.log(JSON.stringify(new ApiError('bad', 404)));
  console.log(JSON.stringify(new Plain()));
  console.log(describe({ headers, list: [1, 'a'] }));
  console.log(describe(new ApiError('x', 500)));
  console.log(describe(controller.signal));
}
