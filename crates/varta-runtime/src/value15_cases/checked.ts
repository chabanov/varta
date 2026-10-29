// Checked JSON parsing and its recoverable errors (Varta-specific API).
export function main(): void {
  const parsed = varta.json.parse<{ name: string; list: (number | null)[]; extra?: string }>(
    '{"list":[1,null,2.5],"name":"n"}'
  );
  console.log(parsed.name + " " + String(parsed.list.length) + " " + String(parsed.extra === undefined));
  console.log(JSON.stringify(parsed));
  try {
    varta.json.parse<{ n: number }>('{"n":"x"}');
  } catch (e) {
    if (e instanceof TypeError) {
      console.log(e.name + ": " + e.message);
    }
  }
  try {
    varta.json.parse<number[]>("[1,");
  } catch (e) {
    if (e instanceof SyntaxError) {
      console.log(e.name);
    }
  }
}
