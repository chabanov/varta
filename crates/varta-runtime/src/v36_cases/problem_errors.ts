// The pilot's error hierarchy: a class holding a problem document whose
// declared fields need conversion, read through getters and subclasses.
interface ProblemDocument {
  type?: string;
  title?: string;
  status?: number;
  detail?: string;
  code?: string;
  correlation_id?: string;
  errors?: Array<{ field?: string; message?: string }>;
  [key: string]: unknown;
}
class UarpError extends Error {
  constructor(message: string, options?: { cause?: unknown }) {
    super(message, options);
    this.name = new.target.name;
  }
}
class APIError extends UarpError {
  readonly status: number;
  readonly problem: ProblemDocument;
  readonly code?: string;
  readonly reason?: string;
  constructor(status: number, problem: ProblemDocument, message?: string) {
    super(message ?? formatMessage(status, problem));
    this.status = status;
    this.problem = problem;
    this.code = typeof problem.code === "string" ? problem.code : undefined;
    this.reason = typeof problem.reason === "string" ? problem.reason : undefined;
  }
  get validationErrors(): Array<{ field?: string; message?: string }> {
    return this.problem.errors ?? [];
  }
}
class UnprocessableEntityError extends APIError {}
function formatMessage(status: number, problem: ProblemDocument): string {
  const title = problem.title ?? `HTTP ${status}`;
  const detail = problem.detail ? ` — ${problem.detail}` : "";
  const id = problem.correlation_id;
  const correlation = id ? ` (correlationId: ${id})` : "";
  return `${status} ${title}${detail}${correlation}`;
}
function errorForStatus(status: number, problem: ProblemDocument): APIError {
  switch (status) {
    case 422:
      return new UnprocessableEntityError(status, problem);
    default:
      if (status >= 500) return new APIError(status, problem, "server");
      return new APIError(status, problem);
  }
}
function readProblem(text: string, status: number): ProblemDocument {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return { status, detail: text.slice(0, 20) };
  }
  if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) {
    const body = parsed as Record<string, unknown>;
    if ("title" in body || "errors" in body) return body as ProblemDocument;
    return { status, detail: "no problem keys" };
  }
  return { status, detail: String(parsed) };
}
export function main(): void {
  const text =
    '{"title":"Invalid","status":422,"code":"VALIDATION","reason":"fields","correlation_id":"c-1",' +
    '"errors":[{"field":"name","message":"required"},{"message":"too long"}]}';
  const error = errorForStatus(422, readProblem(text, 422));
  console.log(error.message + " | " + error.name);
  console.log(String(error.code) + " " + String(error.reason) + " " + String(error instanceof UnprocessableEntityError));
  const list = error.validationErrors;
  console.log(String(list.length) + " " + String(list === error.validationErrors));
  for (const item of list) {
    if (item !== undefined) console.log(String(item.field) + ": " + String(item.message));
  }
  const plain = errorForStatus(404, readProblem('{"title":"Missing","reason":7}', 404));
  console.log(plain.message + " " + String(plain.reason) + " " + String(plain.validationErrors.length));
  const html = errorForStatus(502, readProblem("<html>bad gateway</html>", 502));
  console.log(html.message + " " + String(html.problem.detail));
}
