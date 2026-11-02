export function main(): void {
  View.setTitle('Varta password and search fields');
  View.stylesheet(1, '.field { display:block; width:160px; height:40px; min-width:0; min-height:0; padding:4px; border:2px solid #000; border-radius:0; outline:none; background-color:#fff; color:#00f; font-family:"Varta Password Fixture"; font-size:20px; font-weight:normal; font-style:normal; line-height:20px }');
  const password = View.node();
  View.createElement(password, 24);
  View.setAttribute(password, 1, 'password');
  View.setAttribute(password, 2, 'password');
  View.setClass(password, 'field');
  View.setValue(password, 'AФé😀B');
  View.on(password, 2, (event: ViewEvent): void => console.log('password edited'), 0, 0, 0, 0);
  const search = View.node();
  View.createElement(search, 24);
  View.setAttribute(search, 1, 'search');
  View.setAttribute(search, 2, 'search');
  View.setClass(search, 'field');
  View.setValue(search, 'AAAA');
  View.on(search, 2, (event: ViewEvent): void => console.log('search edited'), 0, 0, 0, 0);
  View.insert(1, password, 0);
  View.insert(1, search, 0);
  View.commit();
}
