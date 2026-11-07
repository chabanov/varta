// Engine prelude of checked-f64-utf16-v38 (`declaration_forms`, K9 B): the
// native error constructors `ReferenceError` and `EvalError` (ECMA-262
// §20.5.5.3 and §20.5.5.1) as `Error` subclasses in typed guest code, like
// the URI prelude's `URIError`. The compiler adds it to a module that names
// one of them, and to a module with a binding that code may access in its
// temporal dead zone: such an access throws the `ReferenceError` that
// `referenceError` creates, so `e instanceof ReferenceError` holds as in
// JavaScript.

export class ReferenceError extends Error {
  constructor(message?: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'ReferenceError';
  }
}

export class EvalError extends Error {
  constructor(message?: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = 'EvalError';
  }
}

export function referenceError(message: string): ReferenceError {
  return new ReferenceError(message);
}
