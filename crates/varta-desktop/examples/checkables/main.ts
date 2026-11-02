function choice(kind: string, name: string): number {
  const field = View.node(); View.createElement(field, 24);
  View.setAttribute(field, 2, kind); View.setAttribute(field, 3, name);
  View.setClass(field, 'switch');
  View.on(field, 2, (event: ViewEvent): void => console.log(`${kind} ${event.target} ${event.checked()} ${event.seq}`), 0, 0, 0, 0);
  return field;
}
export function main(): void {
  View.setTitle('Varta checkbox and radio fields');
  View.stylesheet(1, '.switch { display:block; width:32px; height:32px; min-width:0; min-height:0; padding:4px; border:2px solid #000; border-radius:0; outline:none; background-color:#fff; color:#00f } .switch:focus-visible { outline:2px solid #f00; outline-offset:0 } .switch:checked { color:#080 }');
  const check = choice('checkbox','');
  const a = choice('radio','g'); const b = choice('radio','g');
  View.insert(1,check,0); View.insert(1,a,0); View.insert(1,b,0);
  const form = View.node(); View.createElement(form,22); View.insert(1,form,0);
  const other = choice('radio','g'); View.insert(form,other,0);
  View.setChecked(a,true); View.setChecked(other,true); View.commit();
}
