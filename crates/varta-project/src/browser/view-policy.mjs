import { registry } from "../../../varta-common/browser/registry.mjs";

/** View permission decisions contain no DOM or OS calls. */
export function createViewPolicy(manifest, ceiling, grantAllowed) {
  const V = registry.view, encoder = new TextEncoder();
  const grants = new Map(manifest.capabilities.map((grant) => [grant.id, {...grant,patterns:grant.patterns?.slice()}]));
  const parse = (input) => {
    if (typeof input !== "string" || input !== input.toWellFormed() || encoder.encode(input).length > V.limits.valueBytes || /[\u0000-\u0020\u007f-\u009f\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000\\]/u.test(input)) return;
    try { const url = new URL(input); if (["http:", "https:"].includes(url.protocol) && !url.username && !url.password) return url; } catch {}
  };
  const wildcard = (pattern, text) => {
    if (encoder.encode(pattern).length * encoder.encode(text).length > registry.network.maxMatchWork) return false;
    let row = new Uint8Array(text.length + 1); row[0] = 1;
    for (const char of pattern) {
      const next = new Uint8Array(row.length); if (char === "*") next[0] = row[0];
      for (let i = 0; i < text.length; i++) next[i + 1] = char === "*" ? row[i + 1] || next[i] : row[i] && char === text[i]; row = next;
    }
    return Boolean(row[text.length]);
  };
  const match = (patterns, url) => {
    let prompt = false;
    for (const pattern of patterns ?? []) {
      if (pattern === V.capabilities.anyHttps) { prompt ||= url.protocol === "https:"; continue; }
      const allowed = new URL(pattern);
      if (url.origin === allowed.origin && wildcard(allowed.href.slice(allowed.origin.length), url.href.slice(url.origin.length))) return "explicit";
    }
    return prompt ? "prompt" : "denied";
  };
  const request = grants.get(V.capabilities.openUrl), hostPatterns = ceiling?.[V.capabilities.openUrl]?.patterns?.slice(), host = hostPatterns ? {patterns:hostPatterns}:undefined;
  const clipboardCeiling=ceiling===undefined||Object.hasOwn(ceiling,V.capabilities.clipboard)&&ceiling[V.capabilities.clipboard]===true;
  const granted=(id)=>grantAllowed?grantAllowed(id):grants.get(id)?.required!==false;
  const openDecision=(input)=>{
    const url=parse(input);if(!url||!request||!granted(V.capabilities.openUrl))return'denied';
    const requested=match(request.patterns,url),allowed=ceiling===undefined?requested:host?match(host.patterns,url):'denied';
    if(requested==='denied'||allowed==='denied')return'denied';
    return requested==='explicit'&&allowed==='explicit'?'granted':'prompt';
  };
  return {
    linkRequested(input) { const url = parse(input); return Boolean(url && request && match(request.patterns, url) !== "denied"); },
    openAllowed(input) {return openDecision(input)==='granted';},
    openDecision,
    covers(input){const url=parse(input);return Boolean(url&&request&&match(request.patterns,url)!=="denied"&&(ceiling===undefined||host&&match(host.patterns,url)!=="denied"));},
    clipboardAllowed() {
      const id = V.capabilities.clipboard, grant = grants.get(id);
      return grant?.enabled === true && granted(id) && clipboardCeiling;
    },
  };
}
