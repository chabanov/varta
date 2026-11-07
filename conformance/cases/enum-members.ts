enum Status { Draft, Review = 5, Done }
enum Tone { Warm = 'warm', Cold = 'cold' }
enum Bits { Read = 1 << 0, Write = 1 << 1, Both = Read | Write }
function label(status: Status): string {
  switch (status) {
    case Status.Draft: return 'draft';
    case Status.Review: return 'review';
    case Status.Done: return 'done';
  }
}
export function main(): void {
  console.log(String(Status.Draft) + ' ' + String(Status.Review) + ' ' + String(Status.Done));
  console.log(Status[6] + ' ' + Status[Status.Review]);
  const unknownName = Status[1];
  console.log(unknownName === undefined ? 'no member' : unknownName);
  let current = Status.Draft;
  current = Status.Done;
  console.log(label(current) + ' ' + Tone.Cold + ' ' + String(Bits.Both));
}
