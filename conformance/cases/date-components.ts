export function main(): void {
  console.log(Date.UTC(2024, 1, 29, 12));
  const date = new Date(2024, 0, 31, 8, 30);
  date.setMonth(1);
  console.log(date.toISOString());
  console.log(`${date.getFullYear()} ${date.getMonth()} ${date.getDate()} ${date.getDay()} ${date.getHours()}`);
  console.log(date.toUTCString());
  console.log(date.toString());
  console.log(String(date.toJSON()));
  console.log(Date.parse(date.toString()) === date.getTime() ? "round trip" : "lost");
  console.log(new Date(NaN).toString());
}
