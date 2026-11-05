// The server's response helpers (main.ts) as the console's end-to-end routes
// (e2e.ts, stitch.ts) use them.

/** What the routes need from the server's helpers (main.ts). */
export interface Responder {
  header(request: VartaHttpRequest, name: string): string;
  json(status: number, body: string): VartaHttpResponse;
  problem(status: number, title: string, detail: string): VartaHttpResponse;
  stream(body: string): VartaHttpResponse;
  unavailable(body: string): VartaHttpResponse;
  /** Resolves after `ms` (a timer of the mock's own `time.timer` grant). */
  wait(ms: number): Promise<void>;
}
