// V38 `data_limits`: a string of 655360 UTF-16 code units (1.25 MiB, past
// the 1 MiB ceiling of earlier profiles) is serialized into 1.18 MB of
// UTF-8 JSON, past the 1 MiB host result buffer, and parsed back from more
// than 64 KiB of JSON input.
export function main(): void {
  let text = "aé\"\u{1F600}";
  for (let i = 0; i < 17; i++) {
    text = text + text;
  }
  const json = JSON.stringify(text);
  const back = varta.json.parse<string>(json);
  console.log(`${text.length} ${json.length} ${back.length} ${back === text}`);
}
