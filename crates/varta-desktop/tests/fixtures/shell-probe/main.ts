// The shell probe: a view, then one net.request and a KV round-trip under
// whatever the host grants. Shell tests replace the manifest of the built
// artifact; every denial is caught and logged, so the log shows each outcome.
export async function main(): Promise<void> {
  View.setTitle('Varta shell probe');
  const panel = View.node();
  View.createElement(panel, 1);
  View.setStyle(panel, 2, '64px');
  View.setStyle(panel, 3, '32px');
  View.setStyle(panel, 45, '#0f0');
  View.insert(1, panel, 0);
  View.commit();
  const target = varta.result.capture(varta.fs.read('/config/url.txt'));
  if (target.ok) {
    try {
      const response = await fetch(target.value);
      console.log(`net ${response.status} ${await response.text()}`);
    } catch (error) {
      console.log(`net ${error instanceof DOMException ? error.name : 'error'}`);
    }
  } else {
    console.log('net no-url');
  }
  try {
    const kv = await varta.kv.open('/data/app.kv');
    const before = await kv.get<string>(['shell', 'probe']);
    console.log(`kv before ${before.value ?? 'missing'}`);
    await kv.set(['shell', 'probe'], 'stored');
    const after = await kv.get<string>(['shell', 'probe']);
    console.log(`kv after ${after.value ?? 'missing'}`);
    kv.close();
  } catch (error) {
    console.log(`kv ${error instanceof DOMException ? error.name : 'error'}`);
  }
  console.log('probe done');
}
