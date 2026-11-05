// Real Chrome oracle for the opt-in headless geometry section: each fixture
// app runs through `varta dev --view-script --view-geometry` with explicit
// fonts; Chrome rebuilds the final snapshot's tree (tags, attributes, classes,
// field values, cascaded styles) in a closed shadow root with the official
// applier's root and control base rules, loads the same font bytes, and every
// reported border box must match getBoundingClientRect within ±1 CSS px and its
// used display Chrome's computed outer display. A run without --view-geometry
// must print the same snapshot minus the section.
//
// Usage: node scripts/test-headless-geometry.mjs [varta-executable]
// (default target/debug/varta; build it with `cargo build -p varta --locked`).
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtemp, mkdir, readFile, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {registry} from '../crates/varta-common/browser/registry.mjs';
import {withBrowserPage} from './browser-probe.mjs';

const repo = resolve(import.meta.dirname, '..');
const varta = resolve(process.argv[2] ?? join(repo, 'target/debug/varta'));
const FAMILY = 'Varta Structure Fixture';
const FONTS = [
  {path: join(repo, 'crates/varta-view/tests/fixtures/structure-400.ttf'), weight: 400},
  {path: join(repo, 'crates/varta-view/tests/fixtures/structure-700.ttf'), weight: 700},
];
/// Text-level tags whose initial display is inline (the rest are blocks).
const INLINE_TAGS = new Set(['span', 'strong', 'em', 'code', 'a', 'br', 'label']);
const SIGNALS = join(repo, 'crates/varta-runtime/src/view_cases/signals.ts');
/// The freeze criterion's tolerance (docs/development-plan.md, L0 v1 subset).
const TOLERANCE_PX = 1;
const FIELD = `display: block; width: 120px; height: 24px; min-width: 0; min-height: 0; padding: 2px 4px; border: 1px solid #000; font-family: "${FAMILY}"; font-size: 16px; font-weight: 400; font-style: normal; line-height: 18px`;
const BUTTON = `display: flex; width: 40px; height: 24px; min-width: 0; min-height: 0; padding: 0 4px; border: 1px solid #000; font-family: "${FAMILY}"; font-size: 16px; font-weight: 400; font-style: normal; line-height: 20px; justify-content: center; align-items: center`;
const APP = `font-family: "${FAMILY}"; font-size: 16px; line-height: 20px; padding: 8px`;

// Each fixture is a TSX app (with the view tests' signals helper) and its script.
const fixtures = [
  {
    name: 'counter',
    script: {viewport: {width: 320}, inputs: [{input: 'click', target: '#inc'}, {input: 'click', target: '#inc'}]},
    source: `
const CSS = '.app { ${APP} } .row { display: flex; gap: 6px; align-items: center } .btn { ${BUTTON} }';
export function main(): void {
  View.setEffect(effect);
  View.stylesheet(1, CSS);
  const count = new NumberSignal(1);
  View.mount(
    <main class="app">
      <h1>AB</h1>
      <div class="row" id="row">
        <span>A</span>
        <span id="count">{count.get()}</span>
        <strong>B</strong>
        <button id="inc" class="btn" onClick={() => count.set(count.get() + 1)}>A</button>
      </div>
      <p>A <strong>AB</strong> A</p>
    </main>,
  );
}`,
  },
  {
    name: 'list',
    script: {viewport: {width: 360}, inputs: [{input: 'click', target: '#more'}]},
    source: `
const CSS = '.app { ${APP} } .grid { display: grid; grid-template-columns: 1fr 2fr; gap: 4px } .tabs { display: flex; gap: 8px; padding: 0; margin: 0 } .btn { ${BUTTON} } .hidden { display: none }';
export function main(): void {
  View.setEffect(effect);
  View.stylesheet(1, CSS);
  const shown = new NumberSignal(2);
  View.mount(
    <section class="app">
      <h2>AB</h2>
      <ul>
        <li>A</li>
        <li>AB A</li>
        <li>{shown.get() > 2 ? 'A B A' : 'B'}</li>
      </ul>
      <ol>
        <li>1</li>
        <li>2 A</li>
      </ol>
      <div class="tabs"><span>A</span><em>AB</em><label>B</label></div>
      <div class="grid"><span>A</span><strong>B</strong><span>AB</span></div>
      <p>AB<br />A B</p>
      <div class="hidden"><p>A</p></div>
      <button id="more" class="btn" onClick={() => shown.set(shown.get() + 1)}>B</button>
    </section>,
  );
}`,
  },
  {
    name: 'form',
    script: {viewport: {width: 400}, inputs: [{input: 'type', target: '#name', text: 'AB'}, {input: 'click', target: '#agree'}]},
    source: `
const CSS = '.app { ${APP} } .form { display: grid; grid-template-columns: 80px minmax(0, 1fr); gap: 6px 8px; align-items: center } .field { ${FIELD} } .area { ${FIELD}; height: 60px } .box { display: block; width: 16px; height: 16px; min-width: 0; min-height: 0; padding: 0; border: 1px solid #000 } .actions { display: flex; gap: 4px; justify-content: flex-end } .btn { ${BUTTON} }';
export function main(): void {
  View.setEffect(effect);
  View.stylesheet(1, CSS);
  const name = new TextSignal('');
  View.mount(
    <main class="app">
      <h3>A</h3>
      <form class="form">
        <label for="name">AB</label>
        <input id="name" class="field" type="text" value={name.get()} onInput={(event: ViewEvent) => name.set(event.value)} />
        <label for="note">B</label>
        <textarea id="note" class="area" rows="3" />
        <label for="agree">A</label>
        <input id="agree" class="box" type="checkbox" />
        <span>{name.get()}</span>
        <div class="actions"><button class="btn" type="button">A</button><button class="btn" type="submit">B</button></div>
      </form>
    </main>,
  );
}`,
  },
];

/// The pretty snapshot `varta dev --view-script` prints after the guest output.
function snapshotOf(stdout) {
  const start = stdout.startsWith('{') ? 0 : stdout.indexOf('\n{') + 1;
  const end = stdout.lastIndexOf('\n}') + 2;
  assert(start >= 0 && end > start, `no snapshot in output:\n${stdout}`);
  return JSON.parse(stdout.slice(start, end));
}

/// One `id tag` line per element, to locate a node a diagnostic names.
function outline(node, depth = 0) {
  if (node.text !== undefined) return [];
  return [`${'  '.repeat(depth)}${node.id} ${node.tag}${node.class ? '.' + node.class : ''}`, ...(node.children ?? []).flatMap((child) => outline(child, depth + 1))];
}

function dev(directory, extra, plain) {
  const result = spawnSync(varta, ['dev', '--view-script', 'script.json', ...extra], {cwd: directory, encoding: 'utf8'});
  assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}${plain ? outline(plain.tree).join('\n') : ''}`);
  assert(result.stdout.endsWith('varta dev: completed\n'), result.stdout);
  return snapshotOf(result.stdout);
}

async function run(fixture, work) {
  const directory = join(work, fixture.name);
  await mkdir(join(directory, 'src'), {recursive: true});
  await writeFile(join(directory, 'varta.config.json'), JSON.stringify({entry: 'src/main.tsx', target: 'wasm', policy: 'strict', out: 'dist/app', languageProfile: 'checked-f64-utf16-v38', capabilities: {}}));
  await writeFile(join(directory, 'src/signals.ts'), await readFile(SIGNALS));
  await writeFile(join(directory, 'src/main.tsx'), `import { NumberSignal, TextSignal, effect } from './signals.ts';\n${fixture.source}\n`);
  await writeFile(join(directory, 'script.json'), JSON.stringify(fixture.script));
  const plain = dev(directory, []);
  const measured = dev(directory, ['--view-geometry', ...FONTS.flatMap((font) => ['--view-font', font.path, FAMILY])], plain);
  const {geometry, ...rest} = measured;
  assert(geometry, `${fixture.name}: no geometry section`);
  assert.deepEqual(rest, plain, `${fixture.name}: geometry changed the rest of the snapshot`);
  assert.equal(plain.geometry, undefined);
  return {name: fixture.name, tree: measured.tree, geometry};
}

const work = await mkdtemp(join(tmpdir(), 'varta-headless-geometry-'));
let checks = 0;
try {
  const cases = [];
  for (const fixture of fixtures) cases.push(await run(fixture, work));
  const fonts = await Promise.all(FONTS.map(async (font) => ({weight: font.weight, data: [...await readFile(font.path)]})));
  await withBrowserPage('about:blank', async (call) => {
    async function evaluate(expression) {
      const result = await call('Runtime.evaluate', {expression, awaitPromise: true, returnByValue: true});
      assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
      return result.result.value;
    }
    await evaluate(`(async()=>{document.open();document.write("<!doctype html><html><head></head><body></body></html>");document.close();for(const f of ${JSON.stringify(fonts)})document.fonts.add(await new FontFace(${JSON.stringify(FAMILY)},Uint8Array.from(f.data),{weight:String(f.weight)}).load());document.documentElement.style.colorScheme='light';document.body.style.margin='0';})()`);
    for (const fixture of cases) {
      const {viewport, fontSize, boxes} = fixture.geometry;
      await call('Emulation.setDeviceMetricsOverride', {width: viewport.width, height: viewport.height, deviceScaleFactor: 1, mobile: false});
      const actual = await evaluate(`(async()=>{
        document.body.replaceChildren();
        const host=document.createElement('div');host.style.cssText='width:${viewport.width}px;height:${viewport.height}px';document.body.append(host);
        const shadow=host.attachShadow({mode:'closed'}),style=document.createElement('style');
        // The official applier's base rules (crates/varta-project/src/browser/view-dom.mjs).
        style.textContent=':host{all:initial;display:block} [data-l0-root]{${registry.view.defaults.rootStyle}} button,input,textarea,select{box-sizing:border-box;margin:0} textarea{resize:none}';
        shadow.append(style);
        const nodes=new Map();
        function create(n){
          if(n.text!==undefined){const t=document.createTextNode(n.text);nodes.set(n.id,t);return t;}
          const e=document.createElement(n.tag==='root'?'div':n.tag);nodes.set(n.id,e);
          if(n.tag==='root'){e.setAttribute('data-l0-root','');e.style.fontFamily=${JSON.stringify(`"${FAMILY}"`)};e.style.fontSize='${fontSize}px';}
          for(const [name,value] of Object.entries(n.attributes??{}))e.setAttribute(name,value);
          if(n.class)e.className=n.class;
          for(const [name,value] of Object.entries(n.style??{}))e.style.setProperty(name,value);
          for(const c of n.children??[])e.append(create(c));
          if(typeof n.value==='string')e.value=n.value;
          if(typeof n.checked==='boolean')e.checked=n.checked;
          return e;
        }
        shadow.append(create(${JSON.stringify(fixture.tree)}));
        await document.fonts.ready;
        const displayed=[];
        (function walk(e,id){if(getComputedStyle(e).display==='none')return;if(e.tagName!=='BR')displayed.push(id);for(const c of e.children)walk(c,[...nodes].find(([,n])=>n===c)[0]);})(nodes.get(1),1);
        const boxes={};
        for(const [id,e] of nodes)if(e.nodeType===1){const r=e.getBoundingClientRect(),d=getComputedStyle(e).display;boxes[id]={x:r.x,y:r.y,width:r.width,height:r.height,display:d==='list-item'?'block':d};}
        return {displayed,boxes};
      })()`);
      assert.deepEqual(boxes.map((box) => box.id), actual.displayed, `${fixture.name}: displayed elements in document order`);
      checks++;
      let maxDelta = 0;
      for (const box of boxes) {
        const chrome = actual.boxes[box.id];
        assert.equal(box.display, chrome.display, `${fixture.name} node ${box.id}: used display`);
        checks++;
        for (const key of ['x', 'y', 'width', 'height']) {
          const delta = Math.abs(box[key] - chrome[key]);
          assert(delta <= TOLERANCE_PX, `${fixture.name} node ${box.id} ${key}: chrome ${chrome[key]}, varta ${box[key]}`);
          maxDelta = Math.max(maxDelta, delta);
          checks++;
        }
      }
      // Flex/grid items whose tree keeps a text-level display but lay out as blocks.
      const elements = new Map();
      (function index(node) { elements.set(node.id, node); for (const child of node.children ?? []) index(child); })(fixture.tree);
      const blockified = boxes.filter((box) => box.display === 'block' && (elements.get(box.id).style?.display ?? (INLINE_TAGS.has(elements.get(box.id).tag) ? 'inline' : 'block')) === 'inline').length;
      assert(blockified > 0, `${fixture.name}: no blockified flex or grid item`);
      console.log(JSON.stringify({case: fixture.name, boxes: boxes.length, blockified, maxDelta}));
    }
  });
} finally {
  await rm(work, {recursive: true, force: true});
}
console.log(`Headless geometry/Chrome: ${checks} checks passed within ±${TOLERANCE_PX}px`);
