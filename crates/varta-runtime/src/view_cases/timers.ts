// A scripted-session determinism case (v38): typing, Enter and clicks arm
// chains of zero-delay timers that change the tree, next to delayed timers
// that only a `wait` step reaches. Its turn order, output and final tree
// must not depend on how fast the host runs.
export function main(): void {
  const root = View.node();
  View.createElement(root, 1);
  const box = View.node();
  View.createElement(box, 25);
  View.setAttribute(box, 1, 'box');
  View.insert(root, box, 0);
  const lines = View.node();
  View.createElement(lines, 1);
  View.setAttribute(lines, 1, 'log');
  View.insert(root, lines, 0);
  let entries = 0;
  const append = (text: string): void => {
    entries += 1;
    const line = View.node();
    View.createText(line, `${entries}:${text};`);
    View.insert(lines, line, 0);
    View.commit();
  };
  const chain = (label: string, left: number): void => {
    setTimeout(() => {
      append(`${label}${left}`);
      if (left > 1) {
        chain(label, left - 1);
      }
    }, 0);
  };
  View.on(box, 2, (event: ViewEvent): void => {
    console.log(`input ${event.value}`);
    chain('i', 3);
  }, 0, 0, 0, 0);
  View.on(box, 5, (event: ViewEvent): void => {
    console.log(`key ${event.key}`);
    chain('k', 5);
    setTimeout(() => {
      const button = View.node();
      View.createElement(button, 20);
      View.setAttribute(button, 1, 'later');
      View.on(button, 1, (click: ViewEvent): void => {
        console.log('click later');
        chain('c', 4);
      }, 0, 0, 0, 0);
      View.insert(root, button, 0);
      View.commit();
    }, 0);
  }, 0, 0, 0, 0);
  setTimeout(() => append('delayed'), 30);
  setTimeout(() => append('never'), 100000);
  View.mount(root);
  chain('m', 3);
}
