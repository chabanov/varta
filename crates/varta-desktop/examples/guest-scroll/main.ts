export function main(): void {
  View.setTitle('Varta scroll — starting');
  const container = View.node();
  View.createElement(container, 1);
  View.setStyle(container, 2, '140px');
  View.setStyle(container, 3, '60px');
  View.setStyle(container, 39, 'auto');
  View.on(container, 8, (event: ViewEvent): void => {
    console.log(`guest-scroll ${event.scrollTop}`);
    View.setTitle(`Varta scroll — ${event.scrollTop}`);
  }, 0, 0, 0, 0);
  const spacer = View.node();
  View.createElement(spacer, 1);
  View.setStyle(spacer, 3, '120px');
  View.setStyle(spacer, 45, '#f00');
  const target = View.node();
  View.createElement(target, 1);
  View.setStyle(target, 3, '40px');
  View.setStyle(target, 45, '#0f0');
  View.insert(container, spacer, 0);
  View.insert(container, target, 0);
  View.insert(1, container, 0);
  View.command(target, 4, 0);
  View.commit();
}
