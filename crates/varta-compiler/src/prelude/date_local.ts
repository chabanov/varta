// ---- checked-f64-utf16-v38 (`standard_library`, K9 D): the complete
// ECMAScript 2025 `Date` (ECMA-262 §21.4). This text replaces the v37
// `Date` class of the date prelude under v38 (`modules::engine::date_v38`);
// the parser and calendar helpers above are shared.
//
// Local time is the program's time zone, a recorded host input. Without a
// host-granted zone it is UTC (offset 0, named "Coordinated Universal
// Time"), so local and UTC accessors agree and every output is the output of
// Node with `TZ=UTC`. `LocalTime`/`UTC` (§21.4.1.25–26) go through
// `zoneOffset`, the one place a granted zone would enter.

const ZONE_NAME = 'Coordinated Universal Time';
const MINUTES_PER_HOUR = 60;
const YEAR_WINDOW = 1900;
const MAX_TWO_DIGIT_YEAR = 99;
const INVALID_DATE = 'Invalid Date';

// The offset of local time from UTC at time value `time`, in ms: 0 in the
// UTC zone at every time.
function zoneOffset(time: number): number {
  return Number.isNaN(time) ? 0 : 0;
}

// LocalTime(t) (§21.4.1.25).
function localTime(time: number): number {
  return time + zoneOffset(time);
}

// UTC(t) (§21.4.1.26) of a local time value.
function utcTime(local: number): number {
  return local - zoneOffset(local);
}

// ToIntegerOrInfinity of a finite number (the callers check finiteness).
function whole(value: number): number {
  return Math.trunc(value) + 0;
}

// MakeTime (§21.4.1.27).
function makeTime(hour: number, minute: number, second: number, millis: number): number {
  if (!Number.isFinite(hour) || !Number.isFinite(minute) || !Number.isFinite(second) || !Number.isFinite(millis)) {
    return NaN;
  }
  return whole(hour) * MS_PER_HOUR + whole(minute) * MS_PER_MINUTE + whole(second) * MS_PER_SECOND + whole(millis);
}

// MakeDay (§21.4.1.28).
function makeDay(year: number, month: number, date: number): number {
  if (!Number.isFinite(year) || !Number.isFinite(month) || !Number.isFinite(date)) {
    return NaN;
  }
  const y = whole(year);
  const m = whole(month);
  const dt = whole(date);
  const ym = y + Math.floor(m / 12);
  if (!Number.isFinite(ym)) {
    return NaN;
  }
  const mn = m - Math.floor(m / 12) * 12;
  // Years beyond the time value range have no day number that TimeClip
  // would keep.
  if (Math.abs(ym) > 400000) {
    return NaN;
  }
  return daysFromCivil(ym, mn + 1, 1) + dt - 1;
}

// MakeDate (§21.4.1.29).
function makeDate(day: number, time: number): number {
  if (!Number.isFinite(day) || !Number.isFinite(time)) {
    return NaN;
  }
  const tv = day * MS_PER_DAY + time;
  return Number.isFinite(tv) ? tv : NaN;
}

// MakeFullYear (§21.4.1.30): years 0 to 99 mean 1900 to 1999.
function fullYear(year: number): number {
  if (Number.isNaN(year)) {
    return NaN;
  }
  const truncated = Math.trunc(year) + 0;
  return truncated >= 0 && truncated <= MAX_TWO_DIGIT_YEAR ? YEAR_WINDOW + truncated : year;
}

// The calendar fields of a (local or UTC) time value: year, month (0-11),
// date, hours, minutes, seconds, milliseconds, weekday.
function fields(time: number): number[] {
  const days = Math.floor(time / MS_PER_DAY);
  const civil = civilFromDays(days);
  const within = time - days * MS_PER_DAY;
  const weekday = (days + 4) % 7;
  return [
    civil[0],
    civil[1] - 1,
    civil[2],
    Math.floor(within / MS_PER_HOUR),
    Math.floor(within / MS_PER_MINUTE) % 60,
    Math.floor(within / MS_PER_SECOND) % 60,
    within % MS_PER_SECOND,
    weekday < 0 ? weekday + 7 : weekday,
  ];
}

const YEAR = 0;
const MONTH = 1;
const DAY = 2;
const HOURS = 3;
const MINUTES = 4;
const SECONDS = 5;
const MILLIS = 6;
const WEEKDAY = 7;

function dayFromTime(time: number): number {
  return Math.floor(time / MS_PER_DAY);
}

function timeWithinDay(time: number): number {
  return time - dayFromTime(time) * MS_PER_DAY;
}

function monthName(month: number): string {
  return MONTHS.slice(month * 3, month * 3 + 3);
}

function weekdayName(weekday: number): string {
  return WEEKDAYS.slice(weekday * 3, weekday * 3 + 3);
}

// The year of DateString/toUTCString: four digits, a minus sign before a
// negative year.
function yearText(year: number): string {
  return year >= 0 ? pad(year, 4) : `-${pad(-year, 4)}`;
}

// DateString(tv) (§21.4.4.41.2) of a local time value.
function dateString(local: number): string {
  const f = fields(local);
  return `${weekdayName(f[WEEKDAY]!)} ${monthName(f[MONTH]!)} ${pad(f[DAY]!, 2)} ${yearText(f[YEAR]!)}`;
}

// TimeString(tv) (§21.4.4.41.1) of a local time value.
function timeString(local: number): string {
  const f = fields(local);
  return `${pad(f[HOURS]!, 2)}:${pad(f[MINUTES]!, 2)}:${pad(f[SECONDS]!, 2)} GMT`;
}

// TimeZoneString(tv) (§21.4.4.41.3).
function zoneString(time: number): string {
  const offset = zoneOffset(time);
  const minutes = Math.abs(offset) / MS_PER_MINUTE;
  const sign = offset >= 0 ? '+' : '-';
  return `${sign}${pad(Math.floor(minutes / MINUTES_PER_HOUR), 2)}${pad(minutes % MINUTES_PER_HOUR, 2)} (${ZONE_NAME})`;
}

// The legacy forms that toString, toDateString and toUTCString produce
// (§21.4.3.2 requires `Date.parse` to read them back): `Tue Jan 02 2024`,
// optionally followed by ` 03:04:05 GMT+0000 (zone name)`. The weekday is
// not checked, as in V8.
function parseDateString(text: string): number {
  const scan = new Scanner(text);
  let weekday = -1;
  for (let index = 0; index < 7; index++) {
    if (scan.word(WEEKDAYS.slice(index * 3, index * 3 + 3))) {
      weekday = index;
      break;
    }
  }
  if (weekday < 0 || !scan.take(SPACE)) {
    return NaN;
  }
  let month = 0;
  for (let index = 0; index < 12; index++) {
    if (scan.word(MONTHS.slice(index * 3, index * 3 + 3))) {
      month = index + 1;
      break;
    }
  }
  if (month === 0 || !scan.take(SPACE)) {
    return NaN;
  }
  const day = scan.digits(2);
  if (day < 1 || day > 31 || !scan.take(SPACE)) {
    return NaN;
  }
  const negative = scan.take(MINUS);
  let year = scan.digits(4);
  if (year < 0) {
    return NaN;
  }
  year = negative ? -year : year;
  let time = 0;
  let offset = NaN;
  if (scan.take(SPACE)) {
    const hours = scan.digits(2);
    if (hours < 0 || hours > 23 || !scan.take(COLON)) {
      return NaN;
    }
    const minutes = scan.digits(2);
    if (minutes < 0 || minutes > 59 || !scan.take(COLON)) {
      return NaN;
    }
    const seconds = scan.digits(2);
    if (seconds < 0 || seconds > 59) {
      return NaN;
    }
    time = hours * MS_PER_HOUR + minutes * MS_PER_MINUTE + seconds * MS_PER_SECOND;
    if (scan.word(' GMT')) {
      const sign = scan.take(PLUS) ? -1 : scan.take(MINUS) ? 1 : 0;
      const offsetHours = scan.digits(2);
      const offsetMinutes = scan.digits(2);
      if (sign === 0 || offsetHours < 0 || offsetMinutes < 0 || offsetMinutes > 59) {
        return NaN;
      }
      offset = sign * (offsetHours * MS_PER_HOUR + offsetMinutes * MS_PER_MINUTE);
      if (scan.take(SPACE) && !(scan.take(0x28) && text.charCodeAt(text.length - 1) === 0x29)) {
        return NaN;
      }
      scan.at = text.length;
    }
  }
  if (!scan.done()) {
    return NaN;
  }
  const local = daysFromCivil(year, month, day) * MS_PER_DAY + time;
  return clip(Number.isNaN(offset) ? utcTime(local) : local + offset);
}

// The time value of `year, ...rest` as Date.UTC and the component
// constructor read them (§21.4.3.4, §21.4.2.1 step 4), before UTC(t).
function componentTime(year: number, rest: number[]): number {
  const month = rest.length > 0 ? rest[0]! : 0;
  const date = rest.length > 1 ? rest[1]! : 1;
  const hours = rest.length > 2 ? rest[2]! : 0;
  const minutes = rest.length > 3 ? rest[3]! : 0;
  const seconds = rest.length > 4 ? rest[4]! : 0;
  const millis = rest.length > 5 ? rest[5]! : 0;
  return makeDate(makeDay(fullYear(year), month, date), makeTime(hours, minutes, seconds, millis));
}

export class Date {
  private time: number;
  // Date.UTC (§21.4.3.4).
  static UTC(year: number, ...rest: number[]): number {
    return clip(componentTime(year, rest));
  }
  // new Date(), new Date(value), new Date(text), new Date(date) and
  // new Date(year, month, ...rest) in local time (§21.4.2.1).
  constructor(value?: number | string | Date, ...components: number[]) {
    let time = NaN;
    if (components.length > 0) {
      const year = typeof value === 'number' ? value : NaN;
      time = clip(utcTime(componentTime(year, components)));
    } else if (value === undefined) {
      time = clockNow();
    } else if (typeof value === 'string') {
      time = dateParse(value);
    } else if (typeof value === 'number') {
      time = clip(value);
    } else {
      time = value.getTime();
    }
    this.time = time;
  }
  getTime(): number {
    return this.time;
  }
  setTime(time: number): number {
    this.time = clip(time);
    return this.time;
  }
  getTimezoneOffset(): number {
    if (Number.isNaN(this.time)) {
      return NaN;
    }
    return (this.time - localTime(this.time)) / MS_PER_MINUTE;
  }
  private local(field: number): number {
    return Number.isNaN(this.time) ? NaN : fields(localTime(this.time))[field]!;
  }
  private utc(field: number): number {
    return Number.isNaN(this.time) ? NaN : fields(this.time)[field]!;
  }
  getFullYear(): number {
    return this.local(YEAR);
  }
  getMonth(): number {
    return this.local(MONTH);
  }
  getDate(): number {
    return this.local(DAY);
  }
  getDay(): number {
    return this.local(WEEKDAY);
  }
  getHours(): number {
    return this.local(HOURS);
  }
  getMinutes(): number {
    return this.local(MINUTES);
  }
  getSeconds(): number {
    return this.local(SECONDS);
  }
  getMilliseconds(): number {
    return this.local(MILLIS);
  }
  // Annex B §B.2.3.1.
  getYear(): number {
    return this.local(YEAR) - YEAR_WINDOW;
  }
  getUTCFullYear(): number {
    return this.utc(YEAR);
  }
  getUTCMonth(): number {
    return this.utc(MONTH);
  }
  getUTCDate(): number {
    return this.utc(DAY);
  }
  getUTCDay(): number {
    return this.utc(WEEKDAY);
  }
  getUTCHours(): number {
    return this.utc(HOURS);
  }
  getUTCMinutes(): number {
    return this.utc(MINUTES);
  }
  getUTCSeconds(): number {
    return this.utc(SECONDS);
  }
  getUTCMilliseconds(): number {
    return this.utc(MILLIS);
  }
  // The setters (§21.4.4.20–28): `field` is the first field set, `values`
  // the given arguments in order; fields not given keep their value. A NaN
  // time value stays NaN except for setFullYear, which starts from +0.
  private assign(utc: boolean, field: number, values: number[]): number {
    let t = this.time;
    if (Number.isNaN(t)) {
      if (field !== YEAR) {
        return NaN;
      }
      t = 0;
    } else if (!utc) {
      t = localTime(t);
    }
    const f = fields(t);
    for (let index = 0; index < values.length && field + index <= MILLIS; index++) {
      f[field + index] = values[index]!;
    }
    let result = makeDate(makeDay(f[YEAR]!, f[MONTH]!, f[DAY]!), makeTime(f[HOURS]!, f[MINUTES]!, f[SECONDS]!, f[MILLIS]!));
    if (field === DAY || field === MONTH || field === YEAR) {
      result = makeDate(makeDay(f[YEAR]!, f[MONTH]!, f[DAY]!), timeWithinDay(t));
    }
    this.time = clip(utc ? result : utcTime(result));
    return this.time;
  }
  setMilliseconds(millis: number): number {
    return this.assign(false, MILLIS, [millis]);
  }
  setSeconds(seconds: number, ...rest: number[]): number {
    return this.assign(false, SECONDS, [seconds, ...rest]);
  }
  setMinutes(minutes: number, ...rest: number[]): number {
    return this.assign(false, MINUTES, [minutes, ...rest]);
  }
  setHours(hours: number, ...rest: number[]): number {
    return this.assign(false, HOURS, [hours, ...rest]);
  }
  setDate(date: number): number {
    return this.assign(false, DAY, [date]);
  }
  setMonth(month: number, ...rest: number[]): number {
    return this.assign(false, MONTH, [month, ...rest].slice(0, 2));
  }
  setFullYear(year: number, ...rest: number[]): number {
    return this.assign(false, YEAR, [year, ...rest].slice(0, 3));
  }
  setUTCMilliseconds(millis: number): number {
    return this.assign(true, MILLIS, [millis]);
  }
  setUTCSeconds(seconds: number, ...rest: number[]): number {
    return this.assign(true, SECONDS, [seconds, ...rest]);
  }
  setUTCMinutes(minutes: number, ...rest: number[]): number {
    return this.assign(true, MINUTES, [minutes, ...rest]);
  }
  setUTCHours(hours: number, ...rest: number[]): number {
    return this.assign(true, HOURS, [hours, ...rest]);
  }
  setUTCDate(date: number): number {
    return this.assign(true, DAY, [date]);
  }
  setUTCMonth(month: number, ...rest: number[]): number {
    return this.assign(true, MONTH, [month, ...rest].slice(0, 2));
  }
  setUTCFullYear(year: number, ...rest: number[]): number {
    return this.assign(true, YEAR, [year, ...rest].slice(0, 3));
  }
  toISOString(): string {
    if (Number.isNaN(this.time)) {
      throw new RangeError('Invalid time value');
    }
    const f = fields(this.time);
    const year = f[YEAR]!;
    const yearPart =
      year >= 0 && year <= 9999 ? pad(year, 4) : (year < 0 ? '-' : '+') + pad(Math.abs(year), 6);
    return `${yearPart}-${pad(f[MONTH]! + 1, 2)}-${pad(f[DAY]!, 2)}T${pad(f[HOURS]!, 2)}:${pad(f[MINUTES]!, 2)}:${pad(
      f[SECONDS]!,
      2,
    )}.${pad(f[MILLIS]!, 3)}Z`;
  }
  toDateString(): string {
    return Number.isNaN(this.time) ? INVALID_DATE : dateString(localTime(this.time));
  }
  toTimeString(): string {
    if (Number.isNaN(this.time)) {
      return INVALID_DATE;
    }
    return `${timeString(localTime(this.time))}${zoneString(this.time)}`;
  }
  // §21.4.4.43.
  toUTCString(): string {
    if (Number.isNaN(this.time)) {
      return INVALID_DATE;
    }
    const f = fields(this.time);
    return `${weekdayName(f[WEEKDAY]!)}, ${pad(f[DAY]!, 2)} ${monthName(f[MONTH]!)} ${yearText(f[YEAR]!)} ${pad(
      f[HOURS]!,
      2,
    )}:${pad(f[MINUTES]!, 2)}:${pad(f[SECONDS]!, 2)} GMT`;
  }
  // Annex B §B.2.3.3: the same function as toUTCString.
  toGMTString(): string {
    return this.toUTCString();
  }
}

// Date.prototype.toString (§21.4.4.41), valueOf (§21.4.4.44) and toJSON
// (§21.4.4.37). A class member cannot take the names Object.prototype gives
// every object, nor `toJSON`, so `lower::standard` calls these for
// `date.toString()`, `date.valueOf()` and `date.toJSON()`.
export function dateToJson(date: Date): string | null {
  return Number.isNaN(date.getTime()) ? null : date.toISOString();
}

export function dateToString(date: Date): string {
  const time = date.getTime();
  if (Number.isNaN(time)) {
    return INVALID_DATE;
  }
  const local = localTime(time);
  return `${dateString(local)} ${timeString(local)}${zoneString(time)}`;
}

export function dateValueOf(date: Date): number {
  return date.getTime();
}
