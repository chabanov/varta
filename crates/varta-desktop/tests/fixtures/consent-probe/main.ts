// The consent probe: `kv.write` is required (asked at first run) and
// `kv.read`, which opening the database needs, is optional. Main asks for
// `kv.read` outside a user gesture and tries the database; the `ask`
// button asks in a click, `twice` asks twice in one click and `use` only
// tries the database. Every state and outcome is logged.
function button(id: string, label: string): number {
  const node = View.node();
  View.createElement(node, 2);
  View.setAttribute(node, 1, id);
  const text = View.node();
  View.createText(text, label);
  View.insert(node, text, 0);
  View.insert(1, node, 0);
  return node;
}

async function open(label: string): Promise<void> {
  try {
    const kv = await varta.kv.open('/data/app.kv');
    kv.close();
    console.log(`${label} kv ok`);
  } catch (error) {
    console.log(`${label} kv ${error instanceof DOMException ? error.name : 'error'}`);
  }
}

async function ask(label: string): Promise<void> {
  const status = await varta.permissions.request({ name: 'kv.read' });
  console.log(`${label} ${status.state}`);
}

export async function main(): Promise<void> {
  View.setTitle('Varta consent probe');
  View.on(button('ask', 'Ask'), 1, (event: ViewEvent): void => {
    ask('ask').then(() => open('ask'));
  }, 0, 0, 0, 0);
  View.on(button('twice', 'Ask twice'), 1, (event: ViewEvent): void => {
    ask('first');
    ask('second');
  }, 0, 0, 0, 0);
  View.on(button('use', 'Use'), 1, (event: ViewEvent): void => {
    open('use');
  }, 0, 0, 0, 0);
  View.commit();
  await ask('start');
  await open('start');
  console.log('probe ready');
}
