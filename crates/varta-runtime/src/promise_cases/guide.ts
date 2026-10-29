// The two examples of the v34 language guide, combined into one program.
async function fetchPage(n: number): Promise<string> {
  await null;
  if (n === 2) {
    throw new Error(`page ${n} failed`);
  }
  return `page ${n}`;
}

async function load(pages: number): Promise<string[]> {
  const loaded: string[] = [];
  for (let n = 1; n <= pages; n++) {
    try {
      loaded.push(await fetchPage(n));
    } catch (error) {
      if (error instanceof Error) {
        loaded.push(`skipped (${error.message})`);
      }
    }
  }
  return loaded;
}

function chain(): void {
  const ready = new Promise<number>((resolve) => resolve(1));
  ready
    .then((n) => n + 1)
    .then((n) => console.log(`then ${n}`))
    .finally(() => console.log("done"));
  Promise.reject<number>(new Error("lost")).catch((e) => {
    if (e instanceof Error) {
      console.log(`caught ${e.message}`);
    }
    return 0;
  });
  console.log("chain returned");
}

export async function main(): Promise<void> {
  const pages = await load(3);
  console.log(String(pages.length));
  console.log(pages[1]);
  chain();
}
