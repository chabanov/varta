// How a failure reads in a start banner or a timeline note: the error's class
// and message, then the cause it wraps. The SDK wraps a fetch that rejected (a
// capability denial, a refused connection) in `APIConnectionError: Connection
// error`, so without the cause every such failure would read the same.
import { APIConnectionError } from "uarp-sdk";

/** What a thrown value that is not an Error reads as. */
const NOT_AN_ERROR = "a value that is not an Error was thrown";
const CAUSE_OPEN = " (";
const CAUSE_CLOSE = ")";

function errorText(name: string, message: string): string {
  return name + ": " + message;
}

export function describeFailure(error: unknown): string {
  if (error instanceof APIConnectionError) {
    const text = errorText(error.name, error.message);
    const cause = error.cause;
    if (cause instanceof Error) return text + CAUSE_OPEN + errorText(cause.name, cause.message) + CAUSE_CLOSE;
    return text;
  }
  if (error instanceof Error) return errorText(error.name, error.message);
  return NOT_AN_ERROR;
}
