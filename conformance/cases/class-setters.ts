class Range {
  private low = 0;
  private high = 10;
  get span(): number { return this.high - this.low; }
  set span(value: number) { this.high = this.low + Math.max(0, value); }
  set start(value: number) { const span = this.span; this.low = value; this.high = value + span; }
  describe(): string { return String(this.low) + '..' + String(this.high); }
}
export function main(): void {
  const range = new Range();
  range.span = 4;
  range.start = 3;
  range.span *= 2;
  range.span++;
  console.log(range.describe() + ' ' + String(range.span));
}
