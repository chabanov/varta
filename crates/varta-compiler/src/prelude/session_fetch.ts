
// A distinct operation with a host-owned origin binding. Cookies are never
// exposed as a guest value and every redirect is refused by the transport.
export async function sessionFetch(input: string, init?: RequestInit): Promise<Response> {
  const request = new Request(input, init);
  if (request.redirect !== 'error') throw new TypeError('Session redirects are forbidden');
  if (request.streamOf() !== null) throw new TypeError('Session request streams are unsupported');
  return send(request, request.url, request.method, request.bodySource(), 0, true);
}
