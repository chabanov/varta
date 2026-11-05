import {registry} from '../../../varta-common/browser/registry.mjs';
// Adapter for the native-style UTF-16 environment imports. The caller supplies
// ABI constants, checked authority, shared result writer and host budget hooks.
export function createEnvironmentAbi({memory, resultLength, resultStatus, maxInputBytes=registry.environment.maxInputBytes, maxResultBytes=registry.environment.maxResultBytes, statuses=registry.environment.statuses, access, snapshot, chargeBytes, writeResult, emit}) {
  const encoder = new TextEncoder();
  const fail = (status,message) => { resultLength.value=0; resultStatus.value=status; emit(message); return 0; };
  const input = (ptr,len) => {
    if (!Number.isInteger(ptr)||ptr<0||!Number.isInteger(len)||len<0||len>maxInputBytes) return undefined;
    chargeBytes(len);
    const buffer=memory().buffer;
    if (len%2 || ptr+len>buffer.byteLength) return undefined;
    try {
      const text=new TextDecoder('utf-16le',{fatal:true,ignoreBOM:true}).decode(new Uint8Array(buffer,ptr,len));
      return !text.includes('\0') && encoder.encode(text).length<=maxInputBytes ? text : undefined;
    } catch { return undefined; }
  };
  const perform = (id,ptr,len) => {
    resultLength.value=0; resultStatus.value=statuses.ok;
    const name=input(ptr,len);
    if (name===undefined) return fail(statuses.invalid,'<invalid host input>');
    const result=access[id].read(name);
    if (result.status==='denied') return fail(statuses.denied,`<capability denied: env> (${name})`);
    if (result.status==='missing') return fail(statuses.io,'<host I/O error: environment variable not found>');
    if (result.status!=='ok') throw new Error('invalid environment access result');
    return id==='env.has' ? Number(result.value) : writeResult(result.value,pages=>fail(statuses.io,'<host I/O error: failed to grow memory by `'+pages+'`>'));
  };
  const grow = pages => fail(statuses.io,'<host I/O error: failed to grow memory by `'+pages+'`>');
  // Native `env_snapshot`: invalid input first, then the snapshot's own
  // authority, then the UTF-8 and UTF-16 result limits (recoverable statuses).
  const environmentSnapshot = (ptr,len) => {
    resultLength.value=0; resultStatus.value=statuses.ok;
    const name=input(ptr,len);
    if (name===undefined) return fail(statuses.invalid,registry.effects.failures.invalid);
    const result=snapshot.read(name);
    if (result.status==='denied') return fail(statuses.denied,`<capability denied: env> (${name})`);
    if (result.status!=='ok' || typeof result.text!=='string') throw new Error('invalid environment snapshot result');
    if (encoder.encode(result.text).length>maxResultBytes || result.text.length*2>maxResultBytes) return fail(statuses.tooLarge,registry.effects.failures.tooLarge);
    return writeResult(result.text,grow);
  };
  return Object.freeze({env_get16:(ptr,len)=>perform('env.get',ptr,len),env_has16:(ptr,len)=>perform('env.has',ptr,len),[registry.effects.environment.operation]:environmentSnapshot});
}
