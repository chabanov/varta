// V36 declared fields with function fields: optional chains over checked
// declared reads of mixed records, function values stored under the
// `unknown` index, `typeof` on function cells, and checked assertions of
// `unknown` cells holding records with function fields.
interface ProblemDocument {
  title?: string;
  errors?: Array<{ field?: string; message?: string }>;
  meta?: { retry?: { after?: number } };
  [key: string]: unknown;
}

interface Hooks {
  label: string;
  run: (value: number) => number;
}

interface Other {
  label: string;
  size: number;
}

function summary(problem: ProblemDocument): string {
  const first = problem.errors?.[0]?.field ?? 'none';
  const count = problem.errors?.length ?? 0;
  const after = problem.meta?.retry?.after;
  const title = problem.title?.length;
  return first + ' ' + String(count) + ' ' + String(after) + ' ' + String(title);
}

function invoke(value: unknown): string {
  try {
    const hooks = value as Hooks;
    return hooks.label + '=' + String(hooks.run(20));
  } catch (e) {
    return 'rejected ' + (e as Error).name;
  }
}

export function main(): void {
  const parsed: unknown = JSON.parse(
    '{"title":"bad","errors":[{"field":"name"},{"message":"m"}],"meta":{"retry":{"after":5}},"trace":1}',
  );
  if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
    const problem = parsed as Record<string, unknown> as ProblemDocument;
    console.log(summary(problem));
    problem['retry'] = (): number => 7;
    const retry = problem['retry'];
    console.log(typeof retry + ' ' + String(typeof problem.trace));
    if (typeof retry === 'function') console.log('function cell ' + String(retry === problem['retry']));
    console.log(JSON.stringify(problem));
    delete problem.meta;
    console.log(summary(problem));
  }
  const empty: ProblemDocument = {};
  console.log(summary(empty));

  const hooks: Hooks = { label: 'double', run: (value: number): number => value * 2 };
  const other: Other = { label: 'other', size: 1 };
  const cells: unknown[] = [hooks, other, (): number => 1, 'text'];
  for (const cell of cells) {
    console.log(typeof cell + ' ' + invoke(cell));
  }
}
