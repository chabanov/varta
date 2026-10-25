/** Immutable value and status from one captured system call. */
interface VartaResult<T extends number | boolean | string | void> {
  readonly ok: boolean;
  readonly status: number;
  readonly value: T;
}

declare const varta: {
  result: {
    status(): number;
    /** The argument must be a direct privileged varta host call. */
    capture<T extends number | boolean | string | void>(value: T): VartaResult<T>;
  };
  time: { now(): number };
  env: { has(name: string): boolean; print(name: string): void; get(name: string): string };
  fs: { print(path: string): void; read(path: string): string };
  process: { exit(code: number): void };
  net: { fetch(url: string): number; get(url: string): string };
};
