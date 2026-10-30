// Match arrays, their captures and RegExps stay live across collections:
// large string churn grows the heap while they are only reachable through
// locals, an array and a class field.
class Holder {
  readonly pattern: RegExp;
  constructor(pattern: RegExp) {
    this.pattern = pattern;
  }
}
function churn(): number {
  let text = 'x';
  for (let i = 0; i < 14; i++) {
    text = text + text;
  }
  let total = 0;
  for (let i = 0; i < 120; i++) {
    total = total + (text + 'y').length;
  }
  return total;
}
export function main(): void {
  const holder = new Holder(new RegExp('[a-z]+', 'g'));
  const m = /(\w+)@(\w+)\.(\w+)/.exec('mail ann@' + 'example.org');
  const dates: RegExpExecArray[] = [];
  for (let i = 0; i < 200; i++) {
    const d = /(\d+)-(\d+)/.exec('date 2026-10#' + (i < 100 ? 'early' : 'late'));
    if (d !== null) {
      dates.push(d);
    }
    if (i % 100 === 0) {
      churn();
    }
  }
  churn();
  const last = dates[dates.length - 1]!;
  const parts: string[] = m === null ? [] : [m[1] ?? '', m[2] ?? '', m[3] ?? ''];
  console.log(
    `${parts[0]} ${parts[1]} ${parts[2]} ${last[1] ?? ''} ${last[2] ?? ''} ${holder.pattern.source} ${m === null ? 0 : m.length} ${dates.length - 1}`,
  );
}
