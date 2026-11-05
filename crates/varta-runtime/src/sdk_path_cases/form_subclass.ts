// A program's Blob and File subclasses in FormData entries: a method named
// like a former engine helper (`entryName`) is the program's own method and
// never changes the multipart file name; a File subclass instance appended
// without a file name stays the same object. Boundary replaced by `<b>`.
class NamedBlob extends Blob {
  entryName(): string {
    return 'evil';
  }
}
class LabelledFile extends File {
  label(): string {
    return 'labelled';
  }
}
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
export async function main(): Promise<void> {
  const form = new FormData();
  const blob = new NamedBlob(['x']);
  console.log(blob.entryName());
  form.append('k', blob);
  const file = new LabelledFile(['q'], 'f2.txt');
  form.append('f', file);
  const entry = form.get('f');
  console.log(String(entry === file) + ' ' + String(entry instanceof LabelledFile));
  console.log(String(form.get('k') instanceof NamedBlob) + ' ' + String(form.get('k') instanceof File));
  console.log(await wire(form));
}
