// The complete Date (ECMA-262 §21.4) in the UTC zone: Date.UTC, component
// constructors, local and UTC getters and setters, string forms, parsing of
// the forms toString/toUTCString/toISOString produce, and toJSON.
function show(date: Date): string {
  return `${date.getTime()} ${date.toISOString()}`;
}

export function main(): void {
  console.log(Date.UTC(2020, 0, 2, 3, 4, 5, 6));
  console.log(`${Date.UTC(2020)} ${Date.UTC(99, 11)} ${Date.UTC(2020, 13, 35)} ${Date.UTC(2020, -1)} ${Date.UTC(NaN)}`);
  console.log(`${Date.UTC(275760, 8, 13)} ${Date.UTC(275760, 8, 13, 0, 0, 0, 1)}`);
  const date = new Date(2024, 1, 29, 13, 45, 30, 250);
  console.log(show(date));
  console.log(`${date.getFullYear()} ${date.getMonth()} ${date.getDate()} ${date.getDay()} ${date.getHours()} ${date.getMinutes()} ${date.getSeconds()} ${date.getMilliseconds()} ${date.getYear()}`);
  console.log(`${date.getUTCFullYear()} ${date.getUTCMonth()} ${date.getUTCDate()} ${date.getUTCDay()} ${date.getUTCHours()} ${date.getTimezoneOffset()}`);
  console.log(date.toString());
  console.log(date.toDateString());
  console.log(date.toTimeString());
  console.log(date.toUTCString());
  console.log(date.toGMTString());
  console.log(String(date.toJSON()));
  console.log(String(date.valueOf()));

  const copy = new Date(date);
  copy.setDate(31);
  console.log(show(copy));
  copy.setMonth(0, 15);
  console.log(show(copy));
  copy.setFullYear(1999, 11, 31);
  console.log(show(copy));
  copy.setHours(25, 61, 61, 1001);
  console.log(show(copy));
  copy.setMinutes(-1);
  console.log(show(copy));
  copy.setSeconds(30, 500);
  console.log(show(copy));
  copy.setMilliseconds(-1);
  console.log(show(copy));
  copy.setUTCHours(0, 0, 0, 0);
  copy.setUTCDate(1);
  copy.setUTCMonth(5);
  copy.setUTCFullYear(2001);
  console.log(show(copy));
  copy.setUTCMinutes(90);
  copy.setUTCSeconds(3600);
  copy.setUTCMilliseconds(1500);
  console.log(show(copy));
  console.log(copy.setTime(0));

  const invalid = new Date(NaN);
  console.log(`${invalid.toString()} ${invalid.toDateString()} ${invalid.toUTCString()} ${String(invalid.toJSON())}`);
  console.log(`${invalid.getFullYear()} ${invalid.setHours(1)} ${invalid.getTimezoneOffset()}`);
  console.log(show(new Date(NaN).setFullYear(2000) === 946684800000 ? new Date(946684800000) : invalid));
  try {
    invalid.toISOString();
  } catch (error) {
    console.log(`${(error as RangeError).name}: ${(error as RangeError).message}`);
  }

  const old = new Date(-62198755200000);
  console.log(`${old.toString()} | ${old.toUTCString()} | ${old.toISOString()}`);
  const far = new Date(8.64e15);
  console.log(`${far.toString()} | ${far.toISOString()}`);
  console.log(new Date(-1).toISOString());

  for (const text of [
    "2024-02-29",
    "2024-02-29T13:45:30",
    "2024-02-29T13:45:30.250Z",
    "2024-02-29 13:45",
    "2024-02-29T13:45:30+05:30",
    "Thu Feb 29 2024 13:45:30 GMT+0000 (Coordinated Universal Time)",
    "Thu Feb 29 2024 13:45:30 GMT+0200",
    "Thu Feb 29 2024",
    "Thu, 29 Feb 2024 13:45:30 GMT",
    "not a date",
  ]) {
    console.log(`${text} -> ${Date.parse(text)}`);
  }
  const round = new Date(Date.UTC(1969, 6, 20, 20, 17, 40));
  console.log(`${Date.parse(round.toString()) === round.getTime()} ${Date.parse(round.toUTCString()) === round.getTime()} ${Date.parse(round.toISOString()) === round.getTime()}`);
  console.log(JSON.stringify({ when: round.toJSON() }));
}
