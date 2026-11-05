// Query strings as uarp-sdk 0.13.2 builds them (core/transport.ts #send,
// core/util.ts pick/encodeQueryComponent/buildQuery/joinUrl): the generated
// and the per-call query maps are merged by spreads and serialized in
// Object.entries order, which every SDK must match byte for byte.
function pick<T extends object>(source: T | undefined, keys: readonly (keyof T & string)[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (!source) return out;
  for (const key of keys) {
    const value = (source as Record<string, unknown>)[key];
    if (value !== undefined) out[key] = value;
  }
  return out;
}

function encodeQueryComponent(value: string): string {
  let out = '';
  for (const byte of new TextEncoder().encode(value)) {
    const char = String.fromCharCode(byte);
    if (/[A-Za-z0-9\-._~]/.test(char)) out += char;
    else out += '%' + byte.toString(16).toUpperCase().padStart(2, '0');
  }
  return out;
}

function buildQuery(query: Record<string, unknown> | undefined): string {
  if (!query) return '';
  const pairs: string[] = [];
  const add = (key: string, value: unknown): void => {
    pairs.push(`${encodeQueryComponent(key)}=${encodeQueryComponent(String(value))}`);
  };
  for (const [key, value] of Object.entries(query)) {
    if (value === undefined || value === null) continue;
    if (Array.isArray(value)) {
      for (const item of value) {
        if (item !== undefined && item !== null) add(key, item);
      }
    } else if (typeof value === 'object') {
      add(key, JSON.stringify(value));
    } else {
      add(key, value);
    }
  }
  return pairs.length > 0 ? `?${pairs.join('&')}` : '';
}

function joinUrl(baseUrl: string, path: string): string {
  if (/^https?:\/\//i.test(path)) return path;
  return baseUrl.replace(/\/+$/, '') + '/' + path.replace(/^\/+/, '');
}

interface RequestOptions {
  query?: Record<string, unknown>;
}

interface RequestSpec {
  method: string;
  path: string;
  query?: Record<string, unknown>;
  headers?: Record<string, unknown>;
  options?: RequestOptions;
}

interface ListParams {
  limit?: number;
  cursor?: string;
  status?: string;
  'If-Match'?: string;
}

function url(spec: RequestSpec): string {
  const options = spec.options ?? {};
  const query = { ...spec.query, ...options.query };
  return joinUrl('https://api.test/', spec.path) + buildQuery(query);
}

// The SSE variant (Transport.stream): a token appended to the spec's query.
function streamUrl(spec: RequestSpec, token: string): string {
  const query = token ? { ...spec.query, token: token } : spec.query;
  const headers: Record<string, unknown> = { ...spec.headers, Accept: 'text/event-stream' };
  return url({ ...spec, headers, query }) + ' ' + Object.keys(headers).join(',');
}

export function main(): void {
  const params: ListParams = { status: 'run ning', cursor: 'c/1+2', limit: 20, 'If-Match': 'e' };
  console.log(url({ method: 'GET', path: '/api/v1/runs', query: pick(params, ['limit', 'cursor', 'status']) }));
  console.log(
    url({
      method: 'GET',
      path: 'api/v1/runs',
      query: { b: 1, a: 'x', skip: undefined, none: null },
      options: { query: { a: 'override', c: [1, null, 'two'], d: { k: 'v' }, e: true, 'ключ': 'значення ✓' } },
    }),
  );
  console.log(url({ method: 'GET', path: '/x', options: { query: { z: '*+~!()' } } }));
  console.log(url({ method: 'GET', path: 'https://other.test/abs', query: {} }));
  console.log(streamUrl({ method: 'GET', path: '/events', query: { after: 'e-1' }, headers: { 'X-A': 1 } }, 'tok en'));
  console.log(streamUrl({ method: 'GET', path: '/events' }, ''));
}
