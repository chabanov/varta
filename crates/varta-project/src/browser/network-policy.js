// Exact URL/method intersection. This module performs no I/O.
function networkPolicy(manifest, ceiling, optionalRequests = true, grantAllowed, capability = registry.network.capability) {
  const N = registry.network, encoder = new TextEncoder();
  const whitespace = /[\u0000-\u0020\u007f\u0085\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]/u;
  const token = (value) => typeof value === "string" && /^[!#$%&'*+.^_`|~0-9A-Za-z-]+$/.test(value);
  const method = (value) => token(value) && !N.forbiddenMethods.some((name) => name.toLowerCase() === value.toLowerCase());
  const parse = (input) => {
    if (typeof input !== "string" || encoder.encode(input).length > N.maxUrlBytes || whitespace.test(input)) return;
    let url; try { url = new URL(input); } catch { return; }
    if (!["http:", "https:"].includes(url.protocol) || url.username || url.password || url.href.includes("#") || url.pathname.includes("%")) return;
    return url;
  };
  const wildcard = (pattern, text) => {
    if (encoder.encode(pattern).length * encoder.encode(text).length > N.maxMatchWork) return false;
    let row = new Uint8Array(text.length + 1), next = new Uint8Array(row.length); row[0] = 1;
    for (let i = 0; i < pattern.length; i++) {
      next.fill(0);
      if (pattern[i] === "*") {
        next[0] = row[0];
        for (let j = 0; j < text.length; j++) next[j + 1] = row[j + 1] || next[j];
        if (pattern[i + 1] === "*") i++;
      } else for (let j = 0; j < text.length; j++) next[j + 1] = row[j] && pattern[i] === text[j];
      [row, next] = [next, row];
    }
    return Boolean(row[text.length]);
  };
  const match = (pattern, url) => {
    const allowed = new URL(pattern);
    return allowed.origin === url.origin && wildcard(allowed.href.slice(allowed.origin.length), url.href.slice(url.origin.length));
  };
  const scope = (value) => {
    const fail = () => { throw new TypeError("invalid network permission ceiling"); };
    if (!value || typeof value !== "object" || Array.isArray(value) || Object.keys(value).some((key) => !["patterns", "methods", "responseHeaders"].includes(key))) fail();
    const list = (values, max, valid) => {
      if (!Array.isArray(values) || values.length > max) fail();
      const copy = Array.from(values);
      if (!copy.every(valid)) fail(); return copy;
    };
    const patterns = list(value.patterns, N.maxPatterns, (pattern) => {
      if (typeof pattern !== "string" || pattern !== pattern.toWellFormed() || encoder.encode(pattern).length > N.maxPatternBytes || whitespace.test(pattern)) return false;
      const parts = /^(https?):\/\/([^/]+)(?:\/(.*))?$/.exec(pattern);
      if (!parts || /[*?@#]/.test(parts[2]) || /[#%\\]/.test(parts[3] ?? "") || (parts[3] ?? "").split("/").some((part) => part === "." || part === "..")) return false;
      return Boolean(parse(pattern));
    });
    const unique = (values) => { if (new Set(values).size !== values.length) fail(); return values; };
    const methods = value.methods === undefined ? undefined : unique(list(value.methods, N.maxMethods, (name) => method(name) && !N.normalizedMethods.some((known) => known.toLowerCase() === name.toLowerCase() && known !== name)));
    const responseHeaders = value.responseHeaders === undefined ? undefined : unique(list(value.responseHeaders, N.maxVisibleHeaders, (name) => token(name) && name === name.toLowerCase() && name.length <= N.maxHeaderNameBytes && !N.forbiddenResponseHeaders.includes(name)));
    return { patterns, methods, responseHeaders };
  };
  // Package validation happened before bundling. Runtime ceilings take the same
  // bounded validation path and are snapshotted before any request can start.
  const request = manifest.capabilities.find((grant) => grant.id === capability);
  const packageScope = request && scope({ patterns: request.patterns, methods: request.methods, responseHeaders: request.responseHeaders });
  const hostScope = ceiling === undefined ? undefined : Object.hasOwn(ceiling, capability) ? scope(ceiling[capability]) : null;
  const permits = (grant, url, verb) => grant && (verb===undefined || grant.methods === undefined || grant.methods.includes(verb)) && grant.patterns.some((pattern) => match(pattern, url));
  return {
    parse, method, scope, wildcard,
    allows(input, verb) {
      const url = parse(input);
      return Boolean(url && (grantAllowed?grantAllowed(capability):!optionalRequests || request?.required !== false) && permits(packageScope, url, verb) && (hostScope === undefined || permits(hostScope, url, verb)));
    },
    covers(input){const url=parse(input);return Boolean(url&&permits(packageScope,url)&& (hostScope===undefined||permits(hostScope,url)));},
    visibleHeaders() {
      const visible = new Set(packageScope?.responseHeaders ?? N.safelistedHeaders);
      if (!packageScope || hostScope === null) return new Set();
      if (hostScope) { const allowed = new Set(hostScope.responseHeaders ?? N.safelistedHeaders); for (const name of visible) if (!allowed.has(name)) visible.delete(name); }
      return visible;
    },
  };
}
