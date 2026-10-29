// Caught values are `unknown`; Error subclasses carry `cause?: unknown`;
// `instanceof` narrows classes, native errors and Error subclasses.
export class UarpError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = new.target.name;
  }
}
export class APIConnectionError extends UarpError {
  constructor(message = "Connection error", options?: { cause?: unknown }) {
    super(message, options);
  }
}
export class APIError extends UarpError {
  readonly status: number;
  constructor(status: number, message?: string) {
    super(message ?? `HTTP ${status}`);
    this.status = status;
  }
}
class Box {
  label: string;
  constructor(label: string) {
    this.label = label;
  }
}
class LabeledBox extends Box {}
function fail(kind: number): void {
  if (kind === 0) throw new APIError(503);
  if (kind === 1) throw new APIConnectionError("down", { cause: new TypeError("socket") });
  if (kind === 2) throw new RangeError("range");
  if (kind === 3) throw "text";
  if (kind === 4) throw 42;
  if (kind === 5) throw new UarpError("wrapped", { cause: { code: 7 } });
}
function errorMessage(error: unknown): string {
  return error instanceof Error ? error.name + ": " + error.message : String(error);
}
function handle(kind: number): string {
  try {
    fail(kind);
    return "ok";
  } catch (error) {
    if (error instanceof APIError) return "api " + String(error.status) + " " + error.message;
    if (error instanceof APIConnectionError) {
      const cause = error.cause;
      return "connection " + error.message + " cause " + errorMessage(cause);
    }
    if (error instanceof TypeError) return "type " + error.message;
    if (error instanceof Error) return "error " + errorMessage(error);
    return "value " + errorMessage(error);
  }
}
function annotated(): string {
  try {
    fail(4);
  } catch (error: unknown) {
    if (typeof error === "number") return "number " + String(error + 1);
  }
  return "none";
}
function rethrow(kind: number): string {
  try {
    try {
      fail(kind);
    } catch (error) {
      throw error;
    }
  } catch (outer) {
    return "rethrown " + errorMessage(outer);
  }
  return "none";
}
function boxes(value: unknown): string {
  if (value instanceof LabeledBox) return "labeled " + value.label;
  if (value instanceof Box) return "box " + value.label;
  return "not a box";
}
export function main(): void {
  for (let kind = 0; kind <= 6; kind++) {
    console.log(handle(kind));
  }
  console.log(annotated());
  console.log(rethrow(1));
  console.log(rethrow(3));
  const wrapped = new UarpError("outer", { cause: "inner" });
  console.log(String(wrapped.cause === "inner"));
  console.log(String(new UarpError("plain").cause === undefined));
  try {
    fail(5);
  } catch (error) {
    if (error instanceof UarpError) {
      const cause = error.cause as { code: number };
      console.log("code " + String(cause.code));
    }
  }
  console.log(boxes(new LabeledBox("a")));
  console.log(boxes(new Box("b")));
  console.log(boxes({ label: "c" }));
  console.log(boxes("d"));
}
