// Optional class fields and compatible redeclarations of inherited fields.
class UarpError extends Error {
  readonly status?: number;
  readonly cause?: unknown;
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = "UarpError";
  }
}
class WrappedError extends Error {
  cause: unknown;
  constructor(message: string, cause: unknown) {
    super(message);
    this.cause = cause;
  }
}
class Tagged {
  tag: string = "base";
  count?: number;
}
class Child extends Tagged {
  tag: string = "child";
}
export function main(): void {
  const error = new UarpError("failed", { cause: "inner" });
  console.log(error.name + " " + error.message + " " + String(error.cause) + " " + String(error.status));
  const wrapped = new WrappedError("outer", 42);
  console.log(String(wrapped.cause) + " " + String(wrapped instanceof Error));
  const child = new Child();
  console.log(child.tag + " " + String(child.count));
  child.count = 2;
  console.log(String(child.count));
}
