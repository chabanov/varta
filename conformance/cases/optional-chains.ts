// Optional member reads, method calls and calls short circuit on undefined
// and null and evaluate their object once.
interface Node {
  label: string;
  next?: Node;
  visit?: (depth: number) => string;
}
export function main(): void {
  const tail: Node = { label: 'tail', visit: (depth: number): string => `tail@${depth}` };
  const head: Node = { label: 'head', next: tail };
  let reads = 0;
  const get = (): Node | undefined => {
    reads++;
    return head;
  };
  console.log(`${String(get()?.next?.label)} ${String(head.next?.next?.label)} ${reads}`);
  console.log(`${String(head.visit?.(0))} ${String(head.next?.visit?.(1))}`);
  const none: string | undefined = undefined;
  console.log(`${String(none?.length)} ${String('abc'.length)}`);
}
