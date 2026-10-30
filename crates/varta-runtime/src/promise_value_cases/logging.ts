interface Job {
  name: string;
  done: Promise<number>;
}
export async function main(): Promise<void> {
  const pending = new Promise<number>(() => {});
  const fulfilled = Promise.resolve(7);
  const rejected = Promise.reject<number>(new Error("no"));
  rejected.catch(() => 0);
  console.log(pending);
  console.log(fulfilled);
  const text = Promise.resolve("x");
  console.log(text);
  console.log(Promise.resolve());
  const job: Job = { name: "j", done: fulfilled };
  console.log(JSON.stringify(job));
  console.log(JSON.stringify([fulfilled, pending]));
  console.log(typeof job.done);
  const maybe: Promise<number> | number = 3;
  const either: Promise<number> | number = fulfilled;
  console.log((await maybe) + (await either));
  const resolved = Promise.resolve(either);
  if (resolved === fulfilled) {
    console.log("same promise");
  }
  const fresh = Promise.resolve(maybe);
  console.log(await fresh);
  const held: unknown = fulfilled;
  if (held === fulfilled) {
    console.log("held");
  }
  if (held) {
    console.log("truthy");
  }
}
