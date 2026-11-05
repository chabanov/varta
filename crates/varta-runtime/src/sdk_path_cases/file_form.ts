export async function main(): Promise<void> {
  const file = new File(['ab', new Uint8Array([99])], 'notes.txt', { type: 'Text/Plain' });
  console.log(`${file.name} ${String(file.size)} ${file.type} ${String(file instanceof Blob)} ${String(file instanceof File)}`);
  console.log(await file.text());
  const form = new FormData();
  form.append('a', 'one');
  form.append('doc', file);
  form.append('renamed', file, 'other.txt');
  form.append('blob', new Blob(['x']));
  for (const [key, value] of form) {
    console.log(key + ' ' + (typeof value === 'string' ? value : `blob ${String(value.size)}`));
  }
  for (const key of form.keys()) console.log(key);
  const plain = new Blob(['q']);
  console.log(String(plain instanceof File));
}
