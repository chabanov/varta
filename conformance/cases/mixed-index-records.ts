// Mixed index records: declared fields next to a string index signature share
// one own-key order with dynamic keys; declared reads keep their types.
interface Problem {
  title?: string;
  status?: number;
  [key: string]: unknown;
}
function summary(problem: Problem): string {
  const status = problem.status;
  return String(problem.title) + " " + (status === undefined ? "-" : String(status + 1));
}
export function main(): void {
  const problem: Problem = { status: 404, trace: "t-1", "7": true };
  problem.title = "Not Found";
  problem["extra"] = [1];
  console.log(JSON.stringify(problem));
  console.log(summary(problem) + " " + String("trace" in problem));
  delete problem.status;
  delete problem["trace"];
  console.log(JSON.stringify(Object.keys(problem)) + " " + summary(problem));
}
