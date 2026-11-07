export function main(): void {
  const gap = new Date(2024, 2, 31, 3, 30);
  console.log(gap.toString());
  const repeated = new Date(2024, 9, 27, 3, 30);
  console.log(`${repeated.getTime()} ${repeated.getTimezoneOffset()}`);
  const winter = new Date(Date.UTC(2024, 0, 15, 12));
  console.log(`${winter.getHours()} ${winter.toTimeString()}`);
  winter.setMonth(6);
  console.log(`${winter.getHours()} ${winter.getTimezoneOffset()}`);
  console.log(Date.parse('2024-07-01T12:00') - Date.parse('2024-07-01T12:00Z'));
  console.log(new Date(1970, 0, 1).toDateString());
}
