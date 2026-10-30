export function main(): void {
  const inputs = [
    "2021-02-31",
    "2020-02-29T24:00:00Z",
    "2020-01-01T24:00:01Z",
    "2020-01-01T10:00",
    "2020-01-01T10:00:00.123456Z",
    "+020000-01-01T00:00:00Z",
    "-000000-01-01T00:00:00Z",
    "2020",
    "2020-13",
    "Sun, 06 Nov 1994 08:49:37 GMT",
    "2020-01-01T10:00:00+05:30",
    "2020-01-01t10:00:00z",
    "2020-01-01 10:00:00Z",
    "junk",
    "+275760-09-13T00:00:00.001Z",
  ];
  for (let i = 0; i < inputs.length; i++) {
    const text = inputs[i]!;
    console.log(`${text} ${Date.parse(text)}`);
  }
  console.log(new Date(0).toISOString());
  console.log(new Date(-1).toISOString());
  console.log(new Date(8.64e15).toISOString());
  console.log(new Date(-62198755200000).toISOString());
  try {
    new Date(NaN).toISOString();
  } catch (e) {
    if (e instanceof RangeError) console.log(e.message);
  }
  const d = new Date("2024-03-05T06:07:08.009Z");
  console.log(`${d.getUTCDay()} ${d.getUTCFullYear()} ${d.getUTCMonth()} ${d.getUTCDate()} ${d.getUTCHours()} ${d.getUTCMinutes()} ${d.getUTCSeconds()} ${d.getUTCMilliseconds()} ${d.getTime()}`);
}
