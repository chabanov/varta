// A module-level class is initialized after its static elements: code
// outside the class body that reaches it earlier throws ReferenceError.
function build(): string {
  try {
    return new Gadget().label();
  } catch (e) {
    return e instanceof ReferenceError ? e.message : 'other';
  }
}
const early = build();
class Gadget {
  static made: Gadget = new Gadget();
  label(): string { return 'gadget'; }
}
export function main(): void {
  console.log(early + ' | ' + build() + ' ' + Gadget.made.label());
}
