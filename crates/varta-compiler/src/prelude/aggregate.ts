// Engine prelude of checked-f64-utf16-v38 (`standard_library`, K9 D):
// `AggregateError` (ECMA-262 §20.5.7) as an `Error` subclass in typed guest
// code, like the URI prelude's `URIError`. Its `errors` are the given values
// held by `unknown` (TypeScript's lib types them `any[]`); `message` and the
// `cause` option behave as for `Error`.

export class AggregateError extends Error {
  errors: unknown[];
  constructor(errors: unknown[], message?: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'AggregateError';
    this.errors = errors;
  }
}
