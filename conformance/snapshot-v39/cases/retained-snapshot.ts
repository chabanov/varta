interface Answer { n: number; child: { x: number }; note?: string }
export function main(): void {
  const raw: unknown = JSON.parse('{"before":true,"n":1,"child":{"x":2,"extra":3}}');
  const answer = raw as Answer;
  console.log(JSON.stringify(answer));
  console.log(JSON.stringify(Object.keys(answer)));
  const alias = answer;
  answer.child.x = 4;
  const original = raw as Record<string, unknown>;
  const child = original['child'] as Record<string, unknown>;
  child['x'] = 9;
  console.log(alias.child.x, answer === alias, (answer as unknown) === raw);
  console.log(JSON.stringify(answer));
  try { JSON.parse('{"n":1,"child":{"x":2},"extra":3}') as Answer; }
  catch (error) { if (error instanceof TypeError) console.log('strict parse'); }
  try { const bad: unknown = JSON.parse('{"n":"wrong","child":{"x":2}}'); bad as Answer; }
  catch (error) { if (error instanceof TypeError) console.log('checked field'); }
}
