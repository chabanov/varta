// WHATWG "create an entry": a Blob value becomes a File named `blob` (or the
// given file name), a File with a file name becomes a new File of that name,
// and a File without one stays the same object. get, getAll, iteration,
// values(), entries(), set() and the multipart wire bytes (boundary replaced
// by `<b>`) show the entries.
async function wire(form: FormData): Promise<string> {
  const response = new Response(form);
  const type = response.headers.get('content-type') ?? '';
  const marker = 'boundary=';
  const boundary = type.slice(type.indexOf(marker) + marker.length);
  const bytes = new Uint8Array(await response.arrayBuffer());
  let text = '';
  for (const byte of bytes) text += String.fromCharCode(byte);
  return JSON.stringify(text.split(boundary).join('<b>'));
}
function show(value: FormDataEntryValue | null): string {
  if (value === null) return 'null';
  if (typeof value === 'string') return `string ${value}`;
  return `File=${String(value instanceof File)} Blob=${String(value instanceof Blob)} ${value.name} ${value.type} ${String(value.size)}`;
}
export async function main(): Promise<void> {
  const form = new FormData();
  const original = new File(['y'], 'n.txt', { type: 'text/plain' });
  const blob = new Blob(['x'], { type: 'image/png' });
  form.append('blob', blob);
  form.append('file', original, 'other.txt');
  form.append('same', original);
  form.append('named', blob, 'pic.png');
  form.append('text', 'v');
  console.log(String(form.get('blob') instanceof File));
  console.log(show(form.get('blob')));
  console.log(show(form.get('file')));
  console.log(show(form.get('named')));
  console.log(show(form.get('text')));
  console.log(String(form.get('same') === original) + ' ' + String(form.get('file') === original) + ' ' + String(form.get('blob') === blob));
  for (const value of form.getAll('blob')) console.log('all ' + show(value));
  for (const [k, v] of form) console.log(k + ' ' + show(v));
  for (const v of form.values()) console.log('value ' + show(v));
  for (const [k, v] of form.entries()) console.log('entry ' + k + ' ' + show(v));
  form.set('same', new Blob(['zz']), 'set.bin');
  console.log(show(form.get('same')));
  console.log(await wire(form));
}
