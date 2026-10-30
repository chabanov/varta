// v38: a denied timer start is recoverable; setTimeout throws NotAllowedError.
export function main(): void {
  console.log("start");
  try {
    setTimeout(() => console.log("never"), 5);
    console.log("armed");
  } catch (error) {
    console.log(error instanceof Error ? error.name : "other");
  }
  console.log("end");
}
