// Only ABI marshalling and WebAssembly instantiation. The channel policy is
// shared Rust, with separate memory; this module holds no credential value.
export function snapshotSessionBinding(value) {
  if (value === undefined) return undefined;
  if (!value || typeof value !== 'object' || Array.isArray(value)
      || Object.keys(value).some(key => !['apiOrigin','clientOrigin','allowInsecureLoopback'].includes(key))
      || typeof value.apiOrigin !== 'string' || typeof value.clientOrigin !== 'string'
      || value.allowInsecureLoopback !== undefined && typeof value.allowInsecureLoopback !== 'boolean') {
    throw new TypeError('invalid protected session binding');
  }
  return {apiOrigin:value.apiOrigin,clientOrigin:value.clientOrigin,allowInsecureLoopback:value.allowInsecureLoopback === true};
}
export async function createSessionChannel({bytes,binding,pageOrigin,explicitCeiling}) {
  if (binding === undefined || !explicitCeiling) return {allows:()=>false};
  if (!(bytes instanceof Uint8Array)) throw new Error('missing session policy module');
  const module = await WebAssembly.compile(bytes);
  if (WebAssembly.Module.imports(module).length) throw new Error('session policy module has imports');
  const instance = await WebAssembly.instantiate(module,{}), exports = instance.exports;
  const encoder = new TextEncoder();
  const check = (url, method, headers) => {
    const text = [binding.apiOrigin,binding.clientOrigin,pageOrigin,url,method,binding.allowInsecureLoopback?'1':'0',...headers.map(([name])=>name)].join('\n');
    const data = encoder.encode(text);
    const ptr = exports.session_input();
    if (data.length > exports.session_capacity() || ptr+data.length > exports.memory.buffer.byteLength) return false;
    new Uint8Array(exports.memory.buffer,ptr,data.length).set(data);
    return exports.session_check(data.length) === 1;
  };
  if (!check(binding.apiOrigin+'/', 'GET',[])) throw new TypeError('protected session binding does not match this page');
  return {allows:({url,method,headers})=>check(url,method,headers)};
}
