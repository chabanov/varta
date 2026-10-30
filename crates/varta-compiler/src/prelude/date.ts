// Engine prelude of checked-f64-utf16-v37 (`collections`): a `Date` subset in
// typed guest code. The compiler loads it when a program names `Date`; see
// docs/architecture.md (Collections). Time values are UTC milliseconds since
// the epoch. There is no time zone: `Date.parse` reads the ECMAScript date
// time string format (a date-time without an offset as UTC) and the RFC 9110
// IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`), and returns NaN for every
// other form; only the UTC accessors exist. Reading the clock (`Date.now()`,
// `new Date()`) is the host operation `varta.date.now`, which requires the
// exact `time.now` grant; the runtime's permission broker answers a denial
// with NaN (never a clock reading), and the clock throws the recoverable
// `NotAllowedError` `DOMException` the other v37 denials use.

const MS_PER_SECOND = 1000;
const MS_PER_MINUTE = 60000;
const MS_PER_HOUR = 3600000;
const MS_PER_DAY = 86400000;
const MAX_TIME = 8.64e15;
const DAYS_PER_ERA = 146097;
const YEARS_PER_ERA = 400;
const EPOCH_SHIFT = 719468;
const MONTHS = 'JanFebMarAprMayJunJulAugSepOctNovDec';
const WEEKDAYS = 'SunMonTueWedThuFriSat';
const DIGIT_0 = 0x30;
const DIGIT_9 = 0x39;
const ZERO_PAD = '000000';

// TimeClip.
function clip(time: number): number {
  if (!Number.isFinite(time) || Math.abs(time) > MAX_TIME) {
    return NaN;
  }
  return Math.trunc(time) + 0;
}

// Days since the epoch of a proleptic Gregorian date (`day` may overflow
// its month, as in MakeDay).
function daysFromCivil(year: number, month: number, day: number): number {
  const y = month <= 2 ? year - 1 : year;
  const era = Math.floor(y / YEARS_PER_ERA);
  const yearOfEra = y - era * YEARS_PER_ERA;
  const shifted = month > 2 ? month - 3 : month + 9;
  const dayOfYear = Math.floor((153 * shifted + 2) / 5) + day - 1;
  const dayOfEra =
    yearOfEra * 365 + Math.floor(yearOfEra / 4) - Math.floor(yearOfEra / 100) + dayOfYear;
  return era * DAYS_PER_ERA + dayOfEra - EPOCH_SHIFT;
}

// [year, month (1-12), day] of a day number.
function civilFromDays(days: number): [number, number, number] {
  const z = days + EPOCH_SHIFT;
  const era = Math.floor(z / DAYS_PER_ERA);
  const dayOfEra = z - era * DAYS_PER_ERA;
  const yearOfEra = Math.floor(
    (dayOfEra -
      Math.floor(dayOfEra / 1460) +
      Math.floor(dayOfEra / 36524) -
      Math.floor(dayOfEra / (DAYS_PER_ERA - 1))) /
      365,
  );
  const dayOfYear = dayOfEra - (365 * yearOfEra + Math.floor(yearOfEra / 4) - Math.floor(yearOfEra / 100));
  const shifted = Math.floor((5 * dayOfYear + 2) / 153);
  const day = dayOfYear - Math.floor((153 * shifted + 2) / 5) + 1;
  const month = shifted < 10 ? shifted + 3 : shifted - 9;
  return [yearOfEra + era * YEARS_PER_ERA + (month <= 2 ? 1 : 0), month, day];
}

// Reads digits and fixed punctuation of a date string.
class Scanner {
  text: string;
  at: number;
  constructor(text: string) {
    this.text = text;
    this.at = 0;
  }
  done(): boolean {
    return this.at >= this.text.length;
  }
  peek(): number {
    return varta.text.unit(this.text, this.at);
  }
  // Consume `unit` (a code unit) if it is next.
  take(unit: number): boolean {
    if (!this.done() && this.peek() === unit) {
      this.at++;
      return true;
    }
    return false;
  }
  // Exactly `count` decimal digits, or -1.
  digits(count: number): number {
    let value = 0;
    for (let index = 0; index < count; index++) {
      if (this.done()) {
        return -1;
      }
      const unit = this.peek();
      if (unit < DIGIT_0 || unit > DIGIT_9) {
        return -1;
      }
      value = value * 10 + (unit - DIGIT_0);
      this.at++;
    }
    return value;
  }
  // Consume `word` if it is next.
  word(word: string): boolean {
    if (this.text.slice(this.at, this.at + word.length) !== word) {
      return false;
    }
    this.at += word.length;
    return true;
  }
}

const PLUS = 0x2b;
const MINUS = 0x2d;
const COLON = 0x3a;
const DOT = 0x2e;
const COMMA = 0x2c;
const SPACE = 0x20;

// The ECMAScript date time string format, with V8's lowercase `t`/`z`, a
// space separator, any number of fraction digits and day overflow.
function parseIso(text: string): number {
  const scan = new Scanner(text);
  let year = 0;
  if (scan.take(PLUS) || (scan.peek() === MINUS && scan.take(MINUS))) {
    const negative = varta.text.unit(text, 0) === MINUS;
    year = scan.digits(6);
    if (year < 0 || (negative && year === 0)) {
      return NaN;
    }
    year = negative ? -year : year;
  } else {
    year = scan.digits(4);
    if (year < 0) {
      return NaN;
    }
  }
  let month = 1;
  let day = 1;
  if (scan.take(MINUS)) {
    month = scan.digits(2);
    if (month < 1 || month > 12) {
      return NaN;
    }
    if (scan.take(MINUS)) {
      day = scan.digits(2);
      if (day < 1 || day > 31) {
        return NaN;
      }
    }
  }
  let time = 0;
  if (!scan.done()) {
    if (!(scan.take(0x54) || scan.take(0x74) || scan.take(SPACE))) {
      return NaN;
    }
    const hours = scan.digits(2);
    if (hours < 0 || hours > 24 || !scan.take(COLON)) {
      return NaN;
    }
    const minutes = scan.digits(2);
    if (minutes < 0 || minutes > 59) {
      return NaN;
    }
    let seconds = 0;
    let millis = 0;
    if (scan.take(COLON)) {
      seconds = scan.digits(2);
      if (seconds < 0 || seconds > 59) {
        return NaN;
      }
      if (scan.take(DOT)) {
        let scale = 100;
        let fraction = 0;
        let any = false;
        while (!scan.done() && scan.peek() >= DIGIT_0 && scan.peek() <= DIGIT_9) {
          fraction += (scan.peek() - DIGIT_0) * scale;
          scale /= 10;
          any = true;
          scan.at++;
        }
        if (!any) {
          return NaN;
        }
        millis = Math.floor(fraction);
      }
    }
    if (hours === 24 && (minutes !== 0 || seconds !== 0 || millis !== 0)) {
      return NaN;
    }
    time = hours * MS_PER_HOUR + minutes * MS_PER_MINUTE + seconds * MS_PER_SECOND + millis;
    if (!scan.done()) {
      if (scan.take(0x5a) || scan.take(0x7a)) {
        // UTC.
      } else {
        const sign = scan.take(PLUS) ? -1 : scan.take(MINUS) ? 1 : 0;
        const offsetHours = scan.digits(2);
        if (sign === 0 || offsetHours < 0 || offsetHours > 23 || !scan.take(COLON)) {
          return NaN;
        }
        const offsetMinutes = scan.digits(2);
        if (offsetMinutes < 0 || offsetMinutes > 59) {
          return NaN;
        }
        time += sign * (offsetHours * MS_PER_HOUR + offsetMinutes * MS_PER_MINUTE);
      }
    }
  }
  if (!scan.done()) {
    return NaN;
  }
  return clip(daysFromCivil(year, month, day) * MS_PER_DAY + time);
}

// The RFC 9110 IMF-fixdate `Sun, 06 Nov 1994 08:49:37 GMT`.
function parseImf(text: string): number {
  const scan = new Scanner(text);
  let weekday = -1;
  for (let index = 0; index < 7; index++) {
    if (scan.word(WEEKDAYS.slice(index * 3, index * 3 + 3))) {
      weekday = index;
      break;
    }
  }
  if (weekday < 0 || !scan.take(COMMA) || !scan.take(SPACE)) {
    return NaN;
  }
  const day = scan.digits(2);
  if (day < 1 || day > 31 || !scan.take(SPACE)) {
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
  const year = scan.digits(4);
  if (year < 0 || !scan.take(SPACE)) {
    return NaN;
  }
  const hours = scan.digits(2);
  if (hours < 0 || hours > 23 || !scan.take(COLON)) {
    return NaN;
  }
  const minutes = scan.digits(2);
  if (minutes < 0 || minutes > 59 || !scan.take(COLON)) {
    return NaN;
  }
  const seconds = scan.digits(2);
  if (seconds < 0 || seconds > 59 || !scan.word(' GMT') || !scan.done()) {
    return NaN;
  }
  return clip(
    daysFromCivil(year, month, day) * MS_PER_DAY +
      hours * MS_PER_HOUR +
      minutes * MS_PER_MINUTE +
      seconds * MS_PER_SECOND,
  );
}

export function dateParse(text: string): number {
  const iso = parseIso(text);
  return Number.isNaN(iso) ? parseImf(text) : iso;
}

const NOT_ALLOWED = 'NotAllowedError';

// The wall clock in whole milliseconds, or a `NotAllowedError` when the
// `time.now` grant is missing.
function clockNow(): number {
  const time = varta.date.now();
  if (Number.isNaN(time)) {
    throw new DOMException('<capability denied: time.now> (Date.now)', NOT_ALLOWED);
  }
  return time;
}

export function dateNow(): number {
  return clockNow();
}

function pad(value: number, width: number): string {
  const digits = String(value);
  return ZERO_PAD.slice(0, Math.max(width - digits.length, 0)) + digits;
}

export class Date {
  private time: number;
  constructor(value?: number | string) {
    let time = NaN;
    if (value === undefined) {
      time = clockNow();
    } else if (typeof value === 'string') {
      time = dateParse(value);
    } else {
      time = clip(value);
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
  private part(index: number): number {
    if (Number.isNaN(this.time)) {
      return NaN;
    }
    const days = Math.floor(this.time / MS_PER_DAY);
    const civil = civilFromDays(days);
    const within = this.time - days * MS_PER_DAY;
    if (index === 0) {
      return civil[0];
    }
    if (index === 1) {
      return civil[1] - 1;
    }
    if (index === 2) {
      return civil[2];
    }
    if (index === 3) {
      return Math.floor(within / MS_PER_HOUR);
    }
    if (index === 4) {
      return Math.floor(within / MS_PER_MINUTE) % 60;
    }
    if (index === 5) {
      return Math.floor(within / MS_PER_SECOND) % 60;
    }
    if (index === 6) {
      return within % MS_PER_SECOND;
    }
    // Day of the week: the epoch was a Thursday.
    const weekday = (days + 4) % 7;
    return weekday < 0 ? weekday + 7 : weekday;
  }
  getUTCFullYear(): number {
    return this.part(0);
  }
  getUTCMonth(): number {
    return this.part(1);
  }
  getUTCDate(): number {
    return this.part(2);
  }
  getUTCHours(): number {
    return this.part(3);
  }
  getUTCMinutes(): number {
    return this.part(4);
  }
  getUTCSeconds(): number {
    return this.part(5);
  }
  getUTCMilliseconds(): number {
    return this.part(6);
  }
  getUTCDay(): number {
    return this.part(7);
  }
  toISOString(): string {
    if (Number.isNaN(this.time)) {
      throw new RangeError('Invalid time value');
    }
    const year = this.part(0);
    const yearText =
      year >= 0 && year <= 9999 ? pad(year, 4) : (year < 0 ? '-' : '+') + pad(Math.abs(year), 6);
    return `${yearText}-${pad(this.part(1) + 1, 2)}-${pad(this.part(2), 2)}T${pad(this.part(3), 2)}:${pad(
      this.part(4),
      2,
    )}:${pad(this.part(5), 2)}.${pad(this.part(6), 3)}Z`;
  }
}
