interface Ev { data: string; json<T>(): T; size: (n: number) => number }
function make(data: string): Ev {
  return { data: data, json<T>(): T { return JSON.parse(data) as T; }, size: (n: number): number => n + data.length };
}
interface RunAdded { run_id: string }
export function main(): void {
  const events: Ev[] = [make("{\"run_id\":\"run-1\"}"), make("{\"run_id\":\"run-2\"}")];
  let at = 0;
  console.log(events[at++].json<RunAdded>().run_id + ' ' + String(at));
  console.log(events[1].json<RunAdded>().run_id);
  console.log(String(events[0].size(1)));
  const all: Ev[][] = [events];
  console.log(all[0][1].json<RunAdded>().run_id);
  try {
    console.log(events[7].json<RunAdded>().run_id);
  } catch (error) {
    console.log(error instanceof TypeError ? 'TypeError' : 'other');
  }
}
