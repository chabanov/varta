// Minimal test reporter for tests executed by Varta itself: one log line per
// case, and a summary line the CLI wrapper reads.

export interface Report {
  passed: number;
  failed: number;
}

export function report(): Report {
  return { passed: 0, failed: 0 };
}

export function equal(r: Report, name: string, actual: string, expected: string): void {
  if (actual === expected) {
    r.passed += 1;
    console.log("ok   " + name);
    return;
  }
  r.failed += 1;
  console.log("FAIL " + name);
  console.log("  expected " + expected);
  console.log("  actual   " + actual);
}

export function summary(r: Report): void {
  console.log(`SUMMARY ${r.passed} passed, ${r.failed} failed`);
}
