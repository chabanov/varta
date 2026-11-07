// A module-level class is initialized after its static elements: code
// outside the class body that reaches it earlier throws ReferenceError,
// also from a static initializer of the class itself through a function;
// inside the body the class is already initialized.
function make(): string {
  try {
    return new Widget().kind();
  } catch (e) {
    return e instanceof ReferenceError ? e.message : 'other';
  }
}
function count(): string {
  try {
    return String(Widget.total);
  } catch (e) {
    return e instanceof ReferenceError ? 'ReferenceError' : 'other';
  }
}
function named(): string {
  try {
    const kind: typeof Widget = Widget;
    return kind.name;
  } catch (e) {
    return e instanceof ReferenceError ? 'ReferenceError' : 'other';
  }
}
const early = make();
const earlyCount = count();
const earlyName = named();
class Registry {
  static first: string = make();
}
class Widget {
  static total: number = 2;
  static own: Widget = new Widget();
  static seen: string = count();
  kind(): string { return 'widget'; }
}
const late = make();
export function main(): void {
  console.log(early + ' | ' + earlyCount + ' | ' + earlyName + ' | ' + Registry.first);
  console.log(Widget.seen + ' | ' + late + ' ' + Widget.own.kind() + ' ' + count() + ' ' + named());
}
