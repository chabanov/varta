export function main(): void {
  View.setTitle('Varta text editor');
  const field = View.node();
  View.createElement(field, 25);
  View.setAttribute(field, 1, 'editor');
  View.setStyle(field, 1, 'block');
  View.setStyle(field, 2, '160px');
  View.setStyle(field, 3, '80px');
  View.setStyle(field, 4, '0');
  View.setStyle(field, 5, '0');
  View.setStyle(field, 13, '4px');
  View.setStyle(field, 40, '2px solid #000');
  View.setStyle(field, 44, '0');
  View.setStyle(field, 48, 'none');
  View.setStyle(field, 45, '#fff');
  View.setStyle(field, 46, '#00f');
  View.setStyle(field, 53, '"Varta Fixture 400"');
  View.setStyle(field, 54, '20px');
  View.setStyle(field, 55, 'normal');
  View.setStyle(field, 56, 'normal');
  View.setStyle(field, 57, '20px');
  View.setValue(field, 'AAAA\nA\nAAAA');
  View.on(field, 2, (event: ViewEvent): void => {
    console.log(`edit ${JSON.stringify(event.value)}`);
  }, 0, 0, 0, 0);
  View.mount(field);
}
