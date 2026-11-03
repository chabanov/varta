// Compiled, source-free loader/Worker acceptance for computed CSS variables.
import assert from 'node:assert/strict';
import {mkdtemp, writeFile, readFile, rm, access} from 'node:fs/promises';
import {createServer} from 'node:http';
import {tmpdir} from 'node:os';
import {join, resolve, sep} from 'node:path';
import {pathToFileURL} from 'node:url';
import {spawnSync} from 'node:child_process';
import {registry} from '../crates/varta-common/browser/registry.mjs';

const [binary, modulePath, selection = 'all'] = process.argv.slice(2);
assert(binary && modulePath, 'Usage: node scripts/test-browser-variable-bundle.mjs <varta-executable> <playwright-module> [all|chrome|firefox|webkit]');
assert(['all', 'chrome', 'firefox', 'webkit'].includes(selection), 'unknown browser selection');
const {chromium, firefox, webkit} = await import(pathToFileURL(resolve(modulePath)).href);
const V = registry.view;
const code = (entries, name) => {
  const entry = entries.find(entry => entry.name === name);
  assert(entry, `missing registry entry: ${name}`);
  return entry.code;
};
let empty = ':root{--empty0:var(--missing,);';
for (let i = 1; i < 32; i++) empty += `--empty${i}:var(--empty${i - 1})var(--empty${i - 1});`;
empty += '}';
const initial = empty + ':root{--base:#123;--alias:var(--base);--alpha:.5;--font:"var(--missing)";--fontAlias:VAR(--font);--safe:#111;--self:var(--safe,var(--self));--a:var(--c,#abc);--b:var(--c,var(--a));--c:var(--b)}.parent{width:200px}.target{--base:#abc;display:block;color:VAR(--alias,#def);background-color:var(--b,#789);padding:var(--gap,1px 2px);margin:var(--empty31,7px) 3px;opacity:var(--alpha,2);font-family:VAR(--fontAlias,monospace);width:var(--missing,)auto;border:var(--border,2px solid var(--a,#999));outline-color:var(--self)}';
const changed = initial.replace('--base:#123', '--base:#456');
const source = `export function main():void {
  const button=View.node();View.createElement(button,${code(V.tags, 'button')});View.setAttribute(button,${code(V.attributes, 'id')},'trigger');
  const label=View.node();View.createText(label,'Update variables');View.insert(button,label,0);View.insert(1,button,0);
  const parent=View.node();View.createElement(parent,${code(V.tags, 'div')});View.setClass(parent,'parent');View.insert(1,parent,0);
  const sample=View.node();View.createElement(sample,${code(V.tags, 'span')});View.setAttribute(sample,${code(V.attributes, 'id')},'sample');View.setClass(sample,'target');View.insert(parent,sample,0);
  const text=View.node();View.createText(text,'Computed variables');View.insert(sample,text,0);
  let phase=0;
  View.on(button,${code(V.events, 'click')},(event:ViewEvent):void=>{
    phase++;
    if(phase===1){View.stylesheet(1,${JSON.stringify(changed)});View.setTitle('variables updated');View.commit();console.log('UPDATED');}
    else{View.stylesheet(1,'');View.setStyle(sample,${code(V.css, 'color')},'VAR(--alias,#def)');View.setTitle('variables cleared');View.commit();console.log('CLEARED');}
  },0,0,0,0);
  View.stylesheet(1,${JSON.stringify(initial)});View.setTitle('variables initial');View.commit();console.log('READY');
}`;
const directory = await mkdtemp(join(tmpdir(), 'varta-variable-bundle-'));
let server;
try {
  const sourcePath = join(directory, 'main.ts'), configPath = join(directory, 'varta.config.json');
  await writeFile(sourcePath, source);
  await writeFile(configPath, JSON.stringify({entry: 'main.ts', target: 'wasm', policy: 'strict', out: 'dist', languageProfile: 'checked-f64-utf16-v38', capabilities: {}}));
  const built = spawnSync(resolve(binary), ['build', '--target', 'browser', '--config', configPath], {encoding: 'utf8', timeout: 90000, maxBuffer: 4 * 1024 * 1024});
  assert.equal(built.error, undefined); assert.equal(built.status, 0, built.stderr);
  await rm(sourcePath); await rm(configPath);
  for (const path of [sourcePath, configPath]) await assert.rejects(access(path), {code: 'ENOENT'});
  const root = join(directory, 'web');
  server = createServer(async (request, response) => {
    try {
      const name = new URL(request.url, 'http://loopback').pathname;
      const path = resolve(root, `.${decodeURIComponent(name === '/' ? '/index.html' : name)}`);
      if (!path.startsWith(root + sep)) throw Error('outside bundle');
      const bytes = await readFile(path);
      response.setHeader('Cache-Control', 'no-store');
      response.setHeader('Content-Type', path.endsWith('.js') ? 'text/javascript' : path.endsWith('.html') ? 'text/html' : path.endsWith('.wasm') ? 'application/wasm' : 'application/octet-stream');
      response.end(bytes);
    } catch { response.writeHead(404).end(); }
  });
  await new Promise(done => server.listen(0, '127.0.0.1', done));
  const base = `http://127.0.0.1:${server.address().port}`;
  const channel = process.env.VARTA_CHROMIUM_CHANNEL ?? 'chrome';
  const engines = [['chrome', chromium, channel === 'chromium' ? {chromiumSandbox: true} : {channel, chromiumSandbox: true}], ['firefox', firefox, {}], ['webkit', webkit, {}]];
  for (const [name, type, options] of engines) {
    if (selection !== 'all' && selection !== name) continue;
    const browser = await type.launch({headless: true, timeout: 45000, ...options});
    try {
      const context = await browser.newContext();
      // Retain a host-test inspection handle, preserving the production closed root.
      await context.addInitScript(() => {
        const attach = Element.prototype.attachShadow;
        Element.prototype.attachShadow = function(options) {
          const shadow = attach.call(this, options);
          if (this.parentElement?.id === 'varta-view') globalThis.variableBundleRoot = shadow;
          return shadow;
        };
      });
      const page = await context.newPage(), errors = [], external = [];
      page.on('pageerror', error => errors.push(error.message));
      await context.route('**/*', route => {
        const url = route.request().url();
        if (new URL(url).origin === base) return route.continue();
        external.push(url); return route.abort();
      });
      await page.goto(base + '/');
      const state = () => page.evaluate(() => {
        const output = document.querySelector('#varta-output'), root = globalThis.variableBundleRoot;
        const sample = root?.querySelector('#sample'), computed = sample && getComputedStyle(sample);
        return {status: output?.dataset.status, text: output?.textContent, title: document.title, mode: root?.mode,
          style: computed && Object.fromEntries(['color', 'background-color', 'padding-top', 'padding-right', 'margin-top', 'opacity', 'font-family', 'width', 'border-top-width', 'border-top-color', 'outline-color'].map(property => [property, computed.getPropertyValue(property)]))};
      });
      const wait = async (title, marker) => {
        await page.waitForFunction(({title, marker}) => {
          const output = document.querySelector('#varta-output');
          return output?.dataset.status === 'failed' || document.title === title && output?.dataset.status === 'ready' && output.textContent.includes(marker);
        }, {title, marker}, {timeout: 15000});
        const value = await state(); assert.equal(value.status, 'ready', value.text); assert.equal(value.title, title); return value;
      };
      const first = await wait('variables initial', 'READY');
      assert.equal(first.mode, 'closed'); assert(first.text.includes('READY'));
      // Auto content width: 200 - 2 * (3px margin + 2px padding + 2px border).
      const expected = {'color': 'rgb(17, 34, 51)', 'background-color': 'rgb(119, 136, 153)', 'padding-top': '1px', 'padding-right': '2px', 'margin-top': '3px', 'opacity': '0.5', 'width': '186px', 'border-top-width': '2px', 'border-top-color': 'rgb(170, 187, 204)', 'outline-color': 'rgb(17, 17, 17)'};
      for (const [property, value] of Object.entries(expected)) assert.equal(first.style[property], value, `${name}: initial ${property}`);
      assert.match(first.style['font-family'], /varta_font_[A-Za-z0-9_]+_missing/);
      assert(!first.style['font-family'].includes('var('), `${name}: quoted literal font must stay private`);
      const click = async () => {
        const point = await page.evaluate(() => {const rect = globalThis.variableBundleRoot.querySelector('#trigger').getBoundingClientRect(); return {x: rect.x + rect.width / 2, y: rect.y + rect.height / 2};});
        await page.mouse.click(point.x, point.y);
      };
      await click(); const updated = await wait('variables updated', 'UPDATED');
      assert(updated.text.includes('UPDATED')); assert.equal(updated.style.color, 'rgb(68, 85, 102)');
      assert.equal(updated.style['font-family'], first.style['font-family']);
      for (const [property, value] of Object.entries(expected)) if (property !== 'color') assert.equal(updated.style[property], value, `${name}: updated ${property}`);
      await click(); const cleared = await wait('variables cleared', 'CLEARED');
      assert(cleared.text.includes('CLEARED')); assert.equal(cleared.style.color, 'rgb(221, 238, 255)');
      assert.deepEqual(errors, []); assert.deepEqual(external, []);
      console.log(`${name} ${browser.version()}: source-free compiled variables, initial styles, two trusted turns, alias refresh/removal, empty splices, chosen numeric fallback, cycles and private font passed`);
    } finally { await browser.close(); }
  }
} finally {
  if (server) { server.closeAllConnections(); await new Promise(done => server.close(done)); }
  await rm(directory, {recursive: true, force: true});
}
