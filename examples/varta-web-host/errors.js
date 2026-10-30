// Error classes shared by the host modules.

export class ValidationError extends Error {}
/** A fatal end of the execution (trap, budget, scheduler error). */
export class ExecutionFailure extends Error {}
export class GuestExit extends Error {
  constructor(code) {
    super(`guest exit ${code}`);
    this.code = code;
  }
}
/** A failed privileged call that becomes a result status and a diagnostic line. */
export class Recoverable extends Error {
  constructor(status, message, diagnostic) {
    super(message);
    this.status = status;
    this.diagnostic = diagnostic;
  }
}
