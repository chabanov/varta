function encodeQueryComponent(value: string): string {
  let out = "";
  for (const byte of new TextEncoder().encode(value)) {
    const char = String.fromCharCode(byte);
    if ("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~".includes(char)) out += char;
    else out += "%" + byte.toString(16).toUpperCase().padStart(2, "0");
  }
  return out;
}
export function main(): void {
  const bytes = new Uint8Array(4);
  bytes[0] = 300;
  bytes[1] = -1;
  bytes[2] = 3.7;
  bytes[9] = 5;
  // An out-of-range read is `undefined` where its context admits it (`??`);
  // a plain read throws TypeError under the v37 dense-read rule.
  console.log(`${bytes.length} ${bytes[0]} ${bytes[1]} ${bytes[2]} ${bytes[9] ?? "undefined"}`);
  console.log(bytes.join("-"));
  const copy = new Uint8Array([1, 2, 256]);
  console.log([...copy].map((b) => b.toString(16).padStart(2, "0")).join(""));
  console.log(encodeQueryComponent("a b&é€😀"));
  const text = new TextDecoder().decode(new TextEncoder().encode("héllo €"));
  console.log(text);
  const d = new TextDecoder("utf-8");
  const part1 = d.decode(new Uint8Array([0xe2, 0x82]), { stream: true });
  const part2 = d.decode(new Uint8Array([0xac, 0x41]));
  console.log(`[${part1}][${part2}]`);
  console.log(new TextDecoder().decode(new Uint8Array([0xf0, 0x9f, 0x41, 0xc0, 0x80])));
  try {
    new TextDecoder("utf-8", { fatal: true }).decode(new Uint8Array([0xff]));
  } catch (e) {
    if (e instanceof TypeError) console.log(e.message);
  }
  console.log(btoa("hello"));
  console.log(atob("aGVsbG8="));
  try {
    atob("a");
  } catch (e) {
    if (e instanceof DOMException) console.log(`${e.name}: ${e.message}`);
  }
  const random = crypto.getRandomValues(new Uint8Array(16));
  console.log(`${random.length}`);
  const id = crypto.randomUUID();
  console.log(`${id.length} ${id.charAt(14)} ${id.split("-").length}`);
  const s = copy.slice(1);
  console.log(`${s.length} ${s[0]}`);
  const target = new Uint8Array(5);
  target.set([7, 8], 2);
  console.log(target.join());
  console.log(`${target.indexOf(8)} ${target.includes(9)}`);
}
