//! V40 extends the existing byte Fetch prelude without changing older sources.
use std::sync::OnceLock;
pub(super) fn fetch() -> &'static str {
    static SOURCE: OnceLock<String> = OnceLock::new();
    // Host reset has no guest AbortSignal reason. Fill one only for an abort
    // completion; arbitrary source errors (including undefined) stay exact.
    SOURCE.get_or_init(|| {
        include_str!("../prelude/fetch_bytes.ts")
            .replace("  hops: number,\n): Promise<Response>", "  hops: number,\n  session: boolean = false,\n): Promise<Response>")
            .replace("const id = varta.net.start(descriptor(method, url, request.headers, body));", "const encoded = descriptor(method, url, request.headers, body);\n  const id = session ? varta.sessionHost.start(encoded) : varta.net.start(encoded);")
            .replace("  const kind = varta.net.take(id);", "  const kind = varta.net.take(id);\n  if (kind === KIND_ABORTED && !state.cancelled && state.error === undefined) state.error = new DOMException('Request aborted', 'AbortError');")
            + include_str!("../prelude/session_fetch.ts")
    })
}
