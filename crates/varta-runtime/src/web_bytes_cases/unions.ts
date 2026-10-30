// V38 strict checked conversions to unions of record shapes without a
// literal discriminant: a JSON object converts to the one member it matches
// strictly (no unknown keys, required fields present, values typed); none
// or more than one throws TypeError. TypeScript's `as` is unchecked, so the
// error lines are Varta's own; the other lines match Node 24.
interface FeedbackList {
  feedbacks: { message_id: string; reaction: string }[];
}
interface FeedbackOne {
  reaction: string | null;
  reason?: string;
}
interface Loose {
  reason?: string;
}
type Answer = FeedbackList | FeedbackOne;
function describe(answer: Answer): string {
  if ('feedbacks' in answer) {
    return `list ${answer.feedbacks.length}`;
  }
  return `one ${answer.reaction ?? 'null'} ${answer.reason ?? '-'}`;
}
function convert(text: string): string {
  try {
    return describe(JSON.parse(text) as Answer);
  } catch (error) {
    return error instanceof TypeError ? `TypeError ${error.message}` : 'other';
  }
}
function loose(text: string): string {
  try {
    const value = JSON.parse(text) as FeedbackOne | Loose;
    return 'reason' in value ? `reason ${String(value.reason)}` : 'no reason';
  } catch (error) {
    return error instanceof TypeError ? `TypeError ${error.message}` : 'other';
  }
}
interface Left {
  x?: number;
}
interface Right {
  y?: number;
}
function ambiguous(text: string): string {
  try {
    const value = JSON.parse(text) as Left | Right;
    return 'x' in value ? `left ${String(value.x)}` : 'right';
  } catch (error) {
    return error instanceof TypeError ? `TypeError ${error.message}` : 'other';
  }
}
function viaUnknown(value: unknown): string {
  try {
    return describe(value as Answer);
  } catch (error) {
    return error instanceof TypeError ? `TypeError ${error.message}` : 'other';
  }
}
export function main(): void {
  console.log(convert('{"feedbacks":[{"message_id":"m","reaction":"up"}]}'));
  console.log(convert('{"reaction":null}'));
  console.log(convert('{"reaction":"down","reason":"slow"}'));
  console.log(convert('{"reaction":"down","extra":1}'));
  console.log(convert('{"feedbacks":[],"reaction":"x"}'));
  console.log(convert('[]'));
  console.log(loose('{"reaction":"x"}'));
  console.log(loose('{"reason":"r"}'));
  console.log(loose('{}'));
  console.log(ambiguous('{"x":1}'));
  console.log(ambiguous('{"y":2}'));
  console.log(ambiguous('{}'));
  console.log(viaUnknown(JSON.parse('{"reaction":"up"}')));
  const one: FeedbackOne = { reaction: 'kept' };
  console.log(viaUnknown(one));
}
