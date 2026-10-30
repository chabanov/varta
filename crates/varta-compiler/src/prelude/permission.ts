// Engine prelude of checked-f64-utf16-v37 (`net_request`): the recoverable
// capability denial of asynchronous operations. The compiler adds it to a
// module that names `setTimeout`, `setInterval` or the engine-internal
// unreferenced timeout of `AbortSignal.timeout`, and binds its export under a
// dotted name no source declaration can spell. A timer start the runtime's
// permission broker denies throws this `NotAllowedError`, as browsers report
// a refused permission; `fetch` rejects with the same error. See
// docs/architecture.md (Permission broker).

const NOT_ALLOWED = 'NotAllowedError';

export function permissionDenied(capability: string, target: string): DOMException {
  return new DOMException(`<capability denied: ${capability}> (${target})`, NOT_ALLOWED);
}
