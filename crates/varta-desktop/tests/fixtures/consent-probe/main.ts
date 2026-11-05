// The consent probe: `kv.write` is required (asked at first run) and
// `kv.read`, which opening the database needs, is optional. Main asks for
// `kv.read` outside a user gesture and tries the database; the `ask`
// button asks in a click, `twice` asks twice in one click and `use` only
// tries the database. The optional `ui.open_url` grant is the any-HTTPS
// form, so each activation of the `long` link prompts with its full URL,
// which is longer than a native prompt shows. Every state and outcome is
// logged.
const DIV = 2;
const ANCHOR = 21;
const ID = 1;
const HREF = 12;
// Query characters of the long link: more than a native prompt shows in
// full, fewer than the network URL limit.
const LONG_QUERY = 7000;

function element(tag: number, id: string, label: string): number {
  const node = View.node();
  View.createElement(node, tag);
  View.setAttribute(node, ID, id);
  const text = View.node();
  View.createText(text, label);
  View.insert(node, text, 0);
  View.insert(1, node, 0);
  return node;
}

function button(id: string, label: string): number {
  return element(DIV, id, label);
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
  const link = element(ANCHOR, 'long', 'Docs');
  View.setAttribute(link, HREF, 'https://docs.test/guide?q=' + 'a'.repeat(LONG_QUERY));
  View.onLinkResult((event: ViewEvent): void => console.log(`${event.key} ${event.status}`));
  View.commit();
  await ask('start');
  await open('start');
  console.log('probe ready');
}
