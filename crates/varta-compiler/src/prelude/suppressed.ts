// Engine prelude of checked-f64-utf16-v38 (`node_console` batch, K9 D1):
// `SuppressedError` (ECMA-262 2026 §20.5.8, explicit resource management) as
// an `Error` subclass in typed guest code, like `AggregateError`. `error` is
// the error thrown last (by a disposal), `suppressed` the one it suppressed;
// both are held by `unknown` (TypeScript's lib types them `any`). A
// `message` of `undefined` leaves the instance without an own message, as
// the spec's constructor does; there is no options argument.

export class SuppressedError extends Error {
  error: unknown;
  suppressed: unknown;
  constructor(error: unknown, suppressed: unknown, message?: string) {
    super(message);
    this.name = 'SuppressedError';
    this.error = error;
    this.suppressed = suppressed;
  }
}
