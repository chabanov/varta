export const events: string[] = [];
export function note(text: string): number {
  events.push(text);
  return events.length;
}
export function tick(label: string): Promise<number> {
  return Promise.resolve().then(() => note(label));
}
