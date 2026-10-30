// V36 timers with function fields: timer callbacks read from record, class,
// array and dictionary fields, and closures reachable only through fields of
// values that only pending timer frames hold.
interface Job {
  name: string;
  run: (round: number) => string;
}

class Scheduler {
  jobs: Job[] = [];
  onDone?: (count: number) => void;
  hooks: Record<string, () => void> = {};
  add(name: string, run: (round: number) => string): void {
    this.jobs.push({ name, run });
  }
}

function makeJob(prefix: string, index: number): Job {
  const tag = prefix + '-' + String(index);
  const parts = [tag, 'x'.repeat(index + 1)];
  return {
    name: tag,
    run: (round: number): string => String(parts[0]) + ':' + String(parts[1]) + '@' + String(round),
  };
}

function arm(journal: string[], rounds: number): void {
  // Nothing outside the timer frames refers to the scheduler.
  const scheduler = new Scheduler();
  for (let i = 0; i < 3; i++) {
    const job = makeJob('job', i);
    scheduler.add(job.name, job.run);
  }
  scheduler.onDone = (count: number): void => {
    journal.push('done ' + String(count));
  };
  scheduler.hooks['tick'] = (): void => {
    journal.push('hook tick');
  };
  let round = 0;
  // A closure cannot capture a binding before its initialization (TDZ).
  let interval = 0;
  interval = setInterval(() => {
    round++;
    for (const job of scheduler.jobs) {
      if (job !== undefined) journal.push(job.run(round));
    }
    scheduler.hooks['tick']?.();
    if (round === rounds) {
      clearInterval(interval);
      scheduler.onDone?.(scheduler.jobs.length);
    }
  }, 10);
}

export async function main(): Promise<void> {
  const journal: string[] = [];
  arm(journal, 2);
  const holder = {
    later: (): void => {
      journal.push('holder later');
    },
  };
  setTimeout(holder.later, 15);
  const callbacks: Array<() => void> = [
    (): void => {
      journal.push('element 0');
    },
    (): void => {
      journal.push('element 1');
    },
  ];
  for (const callback of callbacks) {
    if (callback !== undefined) setTimeout(callback, 5);
  }
  await new Promise<void>((resolve) => setTimeout(resolve, 40));
  console.log(JSON.stringify(journal));
}
