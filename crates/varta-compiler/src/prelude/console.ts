// Node's console methods beyond `log` (v38 `console_methods`, K9 D1), as
// Node 24 defines them in `lib/internal/console/constructor.js`.
//
// The compiler loads this module when a program names one of the stateful or
// composite methods (`console.group`, `console.count`, `console.time`,
// `console.table`, ...); every console call of such a program then goes
// through `consoleWrite`, so the group indentation applies to all of them.
// A program that names none of them logs directly and keeps its bytes.
//
// Arguments arrive boxed in an `unknown[]` argument list. The text comes
// from the linked formatter (`varta.console.format`, Node's
// `util.formatWithOptions`) and the console renderer (`varta.console.inspect`
// for `console.dir`, `varta.console.table`); `varta.console.out`, `.error` and
// `.warn` write one standard-output or standard-error line.

/// `kGroupIndentationWidth`.
const GROUP_INDENTATION_WIDTH = 2;
/// The streams of `kWriteToConsole` (standard error split by method).
const STDOUT = 0;
const STDERR_ERROR = 1;
const STDERR_WARN = 2;

let groupIndentation = "";
const counts = new Map<string, number>();
const timers = new Map<string, number>();

/// The `util.inspect` options `console.dir` takes.
export interface InspectOptions {
  depth?: number | null;
  showHidden?: boolean;
  breakLength?: number;
  compact?: boolean | number;
  maxArrayLength?: number | null;
  maxStringLength?: number | null;
  sorted?: boolean;
  numericSeparator?: boolean;
  colors?: boolean;
  customInspect?: boolean;
  getters?: boolean;
  showProxy?: boolean;
}

/// `kWriteToConsole`: the group indentation before every line.
export function consoleWrite(stream: number, text: string): void {
  let line = text;
  if (groupIndentation.length !== 0) {
    line = groupIndentation + line.split("\n").join("\n" + groupIndentation);
  }
  if (stream === STDOUT) {
    varta.console.out(line);
  } else if (stream === STDERR_ERROR) {
    varta.console.error(line);
  } else {
    varta.console.warn(line);
  }
}

/// `process.emitWarning` of the console: one standard-error line, outside
/// the group indentation.
function processWarning(message: string): void {
  varta.console.warn("Warning: " + message);
}

export function consoleLog(args: unknown[]): void {
  consoleWrite(STDOUT, varta.console.format(args));
}
export function consoleError(args: unknown[]): void {
  consoleWrite(STDERR_ERROR, varta.console.format(args));
}
export function consoleWarn(args: unknown[]): void {
  consoleWrite(STDERR_WARN, varta.console.format(args));
}

/// `console.trace`: the stack of a `Trace` error, which has no frames.
export function consoleTrace(args: unknown[]): void {
  const message = varta.console.format(args);
  consoleWrite(STDERR_ERROR, message.length === 0 ? "Trace" : "Trace: " + message);
}

/// `console.assert`.
export function consoleAssert(condition: unknown, args: unknown[]): void {
  if (condition) {
    return;
  }
  if (args.length > 0 && typeof args[0] === "string") {
    args[0] = "Assertion failed: " + (args[0] as string);
  } else {
    args.unshift("Assertion failed");
  }
  consoleWarn(args);
}

/// `console.group` and `console.groupCollapsed`.
export function consoleGroup(args: unknown[]): void {
  if (args.length > 0) {
    consoleLog(args);
  }
  groupIndentation = groupIndentation + " ".repeat(GROUP_INDENTATION_WIDTH);
}
export function consoleGroupEnd(): void {
  groupIndentation = groupIndentation.slice(
    0,
    groupIndentation.length - GROUP_INDENTATION_WIDTH,
  );
}

/// `console.count`.
export function consoleCount(label: string): void {
  const count = counts.get(label);
  const next = count === undefined ? 1 : count + 1;
  counts.set(label, next);
  consoleLog([label + ": " + String(next)]);
}
/// `console.countReset`.
export function consoleCountReset(label: string): void {
  if (!counts.has(label)) {
    processWarning("Count for '" + label + "' does not exist");
    return;
  }
  counts.delete(label);
}

/// `formatTime` of `internal/util/debuglog`.
function pad(value: number): string {
  return String(value).padStart(2, "0");
}
function formatTime(duration: number): string {
  const second = 1000;
  const minute = 60 * second;
  const hour = 60 * minute;
  let ms = duration;
  let hours = 0;
  let minutes = 0;
  let seconds = 0;
  if (ms >= second) {
    if (ms >= minute) {
      if (ms >= hour) {
        hours = Math.floor(ms / hour);
        ms = ms % hour;
      }
      minutes = Math.floor(ms / minute);
      ms = ms % minute;
    }
    seconds = ms / second;
  }
  if (hours !== 0 || minutes !== 0) {
    const parts = seconds.toFixed(3).split(".");
    const whole = parts[0];
    const fraction = parts.length > 1 ? parts[1] : "";
    const res = hours !== 0 ? String(hours) + ":" + pad(minutes) : String(minutes);
    const unit = hours !== 0 ? "h:m" : "";
    return res + ":" + whole.padStart(2, "0") + "." + fraction + " (" + unit + "m:ss.mmm)";
  }
  if (seconds !== 0) {
    return seconds.toFixed(3) + "s";
  }
  return String(Number(ms.toFixed(3))) + "ms";
}

/// `console.time`.
export function consoleTime(label: string): void {
  if (timers.has(label)) {
    processWarning("Label '" + label + "' already exists for console.time()");
    return;
  }
  timers.set(label, varta.date.now());
}
/// `timeLogImpl`: `console.log('%s: %s', label, formatted, ...data)`.
function timeLog(label: string, implementation: string, data: unknown[]): void {
  const start = timers.get(label);
  if (start === undefined) {
    processWarning("No such label '" + label + "' for " + implementation);
    return;
  }
  const args: unknown[] = ["%s: %s", label, formatTime(varta.date.now() - start)];
  for (const value of data) {
    args.push(value);
  }
  consoleLog(args);
}
/// `console.timeEnd`.
export function consoleTimeEnd(label: string): void {
  timeLog(label, "console.timeEnd()", []);
  timers.delete(label);
}
/// `console.timeLog`.
export function consoleTimeLog(label: string, data: unknown[]): void {
  timeLog(label, "console.timeLog()", data);
}

/// `console.dir`: `util.inspect(value, options)` on standard output.
export function consoleDir(value: unknown, options?: InspectOptions): void {
  const request: unknown[] = [
    value,
    options?.depth,
    options?.showHidden,
    options?.breakLength,
    options?.compact,
    options?.maxArrayLength,
    options?.maxStringLength,
    options?.sorted,
    options?.numericSeparator,
  ];
  consoleWrite(STDOUT, varta.console.inspect(request));
}

/// `console.table`: a primitive (or a function) is logged as it is.
export function consoleTable(data: unknown, properties: string[] | undefined): void {
  if (data === null || typeof data !== "object") {
    consoleLog([data]);
    return;
  }
  const request: unknown[] = [data, properties];
  consoleWrite(STDOUT, varta.console.table(request));
}
