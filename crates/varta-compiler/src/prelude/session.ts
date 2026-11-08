// V40 protected server sessions. Only trusted server request Stores possess
// the individual grants and a host-configured request context.
const MAX_SECRET_BYTES = 4096;
const RESULT_OK = 'O';
const RESULT_ANONYMOUS = 'N';
const RESULT_DENIED = 'D';
function checked(text: string): string {
  if (text.startsWith(RESULT_OK)) return text.slice(1);
  if (text === RESULT_DENIED) throw new DOMException('Session operation denied', 'NotAllowedError');
  throw new DOMException('Session operation unavailable in this context', 'InvalidStateError');
}
export function httpSessionGet(): string | null {
  const text = varta.sessionHost.read();
  return text === RESULT_ANONYMOUS ? null : checked(text);
}
export function httpSessionSet(secret: string): void {
  if (new TextEncoder().encode(secret).length > MAX_SECRET_BYTES) throw new RangeError('Session secret exceeds limit');
  checked(varta.sessionHost.write(secret));
}
export function httpSessionClear(): void {
  checked(varta.sessionHost.clear());
}
export function sessionReset(): void { varta.sessionHost.reset(); }
