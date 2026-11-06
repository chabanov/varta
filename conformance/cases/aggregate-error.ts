export function main(): void {
  try {
    throw new AggregateError([new TypeError("a"), "b"], "two failures", { cause: 2 });
  } catch (error) {
    if (error instanceof AggregateError) {
      console.log(`${error.name}: ${error.message} ${error.errors.length} ${String(error.cause)}`);
      console.log(String(error instanceof Error));
      const first = error.errors[0];
      if (first instanceof TypeError) {
        console.log(first.message);
      }
    }
  }
}
