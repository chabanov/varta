// Assigning an inherited static field through a subclass creates the
// subclass's own property (OrdinarySet on the class object); the parent
// keeps its value and the subclass's subclasses inherit the new one.
class Counter {
  static count: number = 0;
  static label: string = 'counter';
  static next(): number {
    return ++this.count;
  }
}
class Pages extends Counter {}
class Chapters extends Pages {}
function bump(kind: typeof Counter): string {
  kind.label += '!';
  return String(kind.next());
}
export function main(): void {
  console.log(bump(Pages) + ' ' + bump(Pages) + ' ' + bump(Counter));
  console.log(String(Counter.count) + ' ' + String(Pages.count) + ' ' + String(Chapters.count));
  console.log(Counter.label + ' ' + Pages.label + ' ' + Chapters.label);
  const keys: string[] = [];
  for (const key in Chapters) keys.push(key);
  console.log(Object.keys(Pages).join(',') + ' | ' + keys.join(','));
}
