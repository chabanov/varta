// The view probe: text in the artifact's regular, bold, italic and
// monospace faces, a copy target and an external link. The copy in main
// runs outside a user gesture; every command result is logged, so the log
// shows each outcome of the host's broker and platform.
function element(tag: number, id: string, className: string, label: string): number {
  const node = View.node();
  View.createElement(node, tag);
  View.setAttribute(node, 1, id);
  View.setClass(node, className);
  const text = View.node();
  View.createText(text, label);
  View.insert(node, text, 0);
  View.insert(1, node, 0);
  return node;
}

export function main(): void {
  View.setTitle('Varta view probe');
  View.stylesheet(1, '.line { display:block; font-size:20px; line-height:24px; color:#000 } .bold { font-weight:700 } .italic { font-style:italic } .mono { font-family:monospace }');
  element(2, 'regular', 'line', 'A A');
  element(2, 'bold', 'line bold', 'A A');
  element(2, 'italic', 'line italic', 'A A');
  element(2, 'mono', 'line mono', 'A A');
  const copy = element(2, 'copy', 'line', 'A');
  View.on(copy, 1, (event: ViewEvent): void => {
    View.copyText('copied by a gesture', (status: number): void => console.log(`click copy ${status}`));
  }, 0, 0, 0, 0);
  const link = element(21, 'docs', 'line', 'A');
  View.setAttribute(link, 12, 'https://docs.test/guide');
  View.onLinkResult((event: ViewEvent): void => console.log(`${event.key} ${event.status}`));
  View.copyText('copied by main', (status: number): void => console.log(`main copy ${status}`));
  View.commit();
}
