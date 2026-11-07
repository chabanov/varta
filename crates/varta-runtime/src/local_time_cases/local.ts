// Local-time Date (ECMA-262 §21.4) in the host's time zone: construction
// from local components, getters and setters across transitions, offsets,
// string forms and parsing of local forms. The expected files are Node
// 24.20.0's output with `TZ` and `LANG` set as `local_time_tests` lists.
function show(label: string, date: Date): void {
  console.log(`${label}: ${date.getTime()} ${date.toString()}`);
}

function fields(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()} ${date.getHours()}:${date.getMinutes()}:${date.getSeconds()}.${date.getMilliseconds()} day ${date.getDay()} offset ${date.getTimezoneOffset()}`;
}

export function main(): void {
  // Local components, including the spring-forward gap and the repeated
  // autumn hour of the northern and southern hemispheres.
  const local: number[][] = [
    [2024, 0, 15, 12, 0],
    [2024, 6, 15, 12, 0],
    [2024, 2, 10, 2, 30],
    [2024, 2, 31, 2, 30],
    [2024, 3, 7, 2, 30],
    [2024, 9, 6, 2, 30],
    [2024, 9, 27, 0, 30],
    [2024, 10, 3, 1, 30],
    [2024, 9, 27, 2, 30],
    [1970, 0, 1, 0, 0],
    [1943, 6, 1, 12, 0],
    [1960, 0, 1, 0, 0],
    [2100, 6, 1, 12, 0],
    [12000, 0, 1, 0, 0],
  ];
  for (const c of local) {
    const date = new Date(c[0]!, c[1]!, c[2]!, c[3]!, c[4]!);
    show(`local ${c.join('/')}`, date);
    console.log(`  ${fields(date)}`);
  }
  // Instants around the 2024 transitions of the zones under test.
  const instants = [
    Date.UTC(2024, 2, 10, 6, 59, 59, 999),
    Date.UTC(2024, 2, 10, 7, 0),
    Date.UTC(2024, 2, 31, 0, 59, 59, 999),
    Date.UTC(2024, 2, 31, 1, 0),
    Date.UTC(2024, 3, 6, 14, 59),
    Date.UTC(2024, 3, 6, 15, 0),
    Date.UTC(2024, 9, 5, 15, 29),
    Date.UTC(2024, 9, 5, 15, 30),
    Date.UTC(2024, 9, 27, 0, 59),
    Date.UTC(2024, 9, 27, 1, 0),
    Date.UTC(2024, 10, 3, 5, 59),
    Date.UTC(2024, 10, 3, 6, 0),
    0,
    -1,
  ];
  for (const t of instants) {
    const date = new Date(t);
    show(`instant ${t}`, date);
    console.log(`  ${fields(date)} | ${date.toDateString()} | ${date.toTimeString()}`);
  }
  // Local mean time before the zone's first transition, and the ends of
  // the time value range (getTimezoneOffset is left out: V8 truncates a
  // fractional-minute offset, ECMA-262 does not; local_time_tests pins it).
  for (const t of [Date.UTC(1800, 5, 1), 8.64e15, -8.64e15]) {
    const date = new Date(t);
    show(`instant ${t}`, date);
    console.log(`  ${date.getHours()}:${date.getMinutes()}:${date.getSeconds()} ${date.toTimeString()}`);
  }
  // Setters in local time keep the other fields and land after a gap.
  const d = new Date(2024, 0, 31, 23, 59, 59, 999);
  d.setMonth(1);
  show('setMonth', d);
  d.setDate(10);
  d.setHours(2, 30);
  show('setHours', d);
  d.setFullYear(2023, 2, 12);
  show('setFullYear', d);
  d.setMinutes(90);
  show('setMinutes', d);
  d.setSeconds(-1);
  show('setSeconds', d);
  d.setMilliseconds(1500);
  show('setMilliseconds', d);
  d.setTime(Date.UTC(2024, 6, 1));
  d.setUTCHours(30);
  show('setUTCHours', d);
  const bad = new Date(NaN);
  console.log(`${bad.getHours()} ${bad.getTimezoneOffset()} ${bad.toString()} ${bad.setHours(1)}`);
  console.log(`${new Date(NaN).setFullYear(2024)}`);
  // Parsing: date-time forms without an offset are local, date-only forms
  // and forms with an offset are not; the toString forms round-trip.
  for (const text of [
    '2024-07-01T12:00',
    '2024-07-01T12:00:30.5',
    '2024-03-10T02:30',
    '2024-07-01',
    '2024-07',
    '2024-07-01T12:00Z',
    '2024-07-01T12:00+02:00',
    '+002024-07-01T00:00',
    'Mon Jul 01 2024',
    'Mon Jul 01 2024 12:00:00',
    'Mon Jul 01 2024 12:00:00 GMT+0200',
    'Mon, 01 Jul 2024 12:00:00 GMT',
  ]) {
    console.log(`parse ${text}: ${Date.parse(text)}`);
  }
  const now = new Date(2024, 4, 5, 6, 7, 8, 9);
  console.log(`round trip ${Date.parse(now.toString()) === now.getTime() - 9} ${Date.parse(now.toDateString())} ${now.toISOString()} ${now.toUTCString()} ${JSON.stringify({ at: now.toJSON() })}`);
  console.log(`${Date.UTC(2024, 0)} ${new Date(99, 0).getFullYear()}`);
}
