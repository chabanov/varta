// Appended only to the trusted development loader, covered by its renewed SRI.
function devBootstrap(runtime) {
  const lifetime = new AbortController();
  let timer;
  const close = () => { clearTimeout(timer); lifetime.abort(); };
  addEventListener('pagehide', close, {once: true});
  const report = (message) => {
    let element = document.querySelector('#varta-dev-errors');
    if (!message) { element?.remove(); return; }
    if (!element) {
      element = document.createElement('pre');
      element.id = 'varta-dev-errors';
      element.setAttribute('role', 'alert');
      document.body.append(element);
    }
    element.textContent = message;
  };
  const revision = new URL(import.meta.url).pathname.split('/')[2];
  const poll = async () => {
    if (lifetime.signal.aborted) return;
    const request = new AbortController();
    const abort = () => request.abort();
    lifetime.signal.addEventListener('abort', abort, {once: true});
    const deadline = setTimeout(abort, 3000);
    try {
      const response = await fetch(new URL('/__varta/status', import.meta.url), {
        signal: request.signal, credentials: 'omit', redirect: 'error', cache: 'no-store',
      });
      if (!response.ok) throw new Error('Development server unavailable');
      const reader = response.body.getReader();
      const decoder = new TextDecoder('utf-8', {fatal: true});
      let text = '', bytes = 0;
      try {
        for (;;) {
          const {done, value} = await reader.read();
          if (done) break;
          bytes += value.byteLength;
          if (bytes > 128 * 1024) throw new Error('Invalid development status');
          text += decoder.decode(value, {stream: true});
        }
        text += decoder.decode();
      } finally { await reader.cancel(); }
      const status = JSON.parse(text);
      if (!status || typeof status.revision !== 'string' || !/^[a-f0-9]{64}$/.test(status.revision)
          || !(status.error === null || typeof status.error === 'string')) throw new Error('Invalid development status');
      if (lifetime.signal.aborted) return;
      report(status.error);
      if (status.revision !== revision) {
        close(); // terminate the owned Worker/requests before navigating
        const entry = runtime.entryPathPrefix && location.pathname.startsWith(runtime.entryPathPrefix + '/')
          ? location.pathname : '/';
        location.replace(new URL(entry, import.meta.url).href);
        return;
      }
    } catch (error) {
      if (lifetime.signal.aborted) return;
      report('Development server unavailable; application stopped.');
      close();
      return;
    } finally {
      clearTimeout(deadline);
      lifetime.signal.removeEventListener('abort', abort);
    }
    timer = setTimeout(poll, 750);
  };
  // Only a public browser path is forwarded. Existing Wasm environment policy
  // still checks env.get; no host process environment is read or published.
  const environmentInputs = runtime.entryPathInput
    ? [[runtime.entryPathInput, location.pathname]] : undefined;
  bootstrap({...runtime, environmentInputs, signal: lifetime.signal});
  timer = setTimeout(poll, 750);
}
