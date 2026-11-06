// AggregateError (ECMA-262 §20.5.7) and Error causes (§20.5.8.1).
class ValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ValidationError";
  }
}

function validate(values: number[]): void {
  const problems: unknown[] = [];
  for (const value of values) {
    if (value < 0) {
      problems.push(new ValidationError(`negative: ${value}`));
    } else if (!Number.isInteger(value)) {
      problems.push(new RangeError(`fraction: ${value}`));
    }
  }
  if (problems.length > 0) {
    throw new AggregateError(problems, `${problems.length} invalid values`, { cause: values.length });
  }
}

export function main(): void {
  try {
    validate([1, -2, 3.5, 4]);
  } catch (error) {
    if (error instanceof AggregateError) {
      console.log(`${error.name}: ${error.message} (${String(error.cause)})`);
      console.log(String(error instanceof Error));
      for (const problem of error.errors) {
        if (problem instanceof Error) {
          console.log(`  ${problem.name}: ${problem.message}`);
        }
      }
    }
  }
  const empty = new AggregateError([]);
  console.log(`[${empty.message}] ${empty.errors.length} ${empty.name}`);
  const mixed = new AggregateError([1, "two", null], "mixed");
  console.log(`${mixed.errors.length} ${typeof mixed.errors[1]} ${mixed.toString()}`);
  console.log(typeof AggregateError);
}
