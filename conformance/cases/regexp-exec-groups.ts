export function main(): void {
  const re = /(?<key>\w+)=(?<value>\w*)/g;
  let text = '';
  let m = re.exec('a=1 b= c=33');
  while (m !== null) {
    const groups = m.groups;
    const key = groups === undefined ? '?' : groups['key'] ?? '?';
    text = text + `${m.index}:${key}:${m[2] ?? '~'}@${re.lastIndex} `;
    m = re.exec('a=1 b= c=33');
  }
  console.log(text + `${re.lastIndex}`);
  console.log(String(/^\p?$/.test('p')) + String(/ſ/iu.test('S')) + String(/./u.exec('😀x')![0].length));
}
