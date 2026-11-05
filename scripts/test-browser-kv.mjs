// Native/browser equivalence of v38 KV storage in the official browser host:
// OPFS containers held through sync access handles and the shared Rust value
// layer compiled to wasm32 (docs/embedding.md, KV storage (v38)).
//
// Each case builds a project with `varta build --target browser`, serves the
// bundle from one loopback origin under /<case>/ and runs it in a real
// browser (one profile for the whole session, so OPFS persists across pages
// and reloads), and runs the same project natively with
// `varta dev --budget-report` (and `--policy` for a host ceiling):
//
// - basic: commit, atomic check failure, KvU64 sums, list, get, a strict
//   decode mismatch and a delete; logs and usage (exact fuel, host calls,
//   microtasks, output bytes, turns) equal native;
// - persistence across reload: the page is reloaded and the program run
//   again over the stored data; equal to native's second run;
// - isolation: a second artifact on the same origin reads the same path and
//   finds nothing, as a fresh native project does;
// - missing grant: every open denied, audited, equal to native;
// - host ceiling: a narrower `kv.*` ceiling denies, equal to native's host
//   policy; an invalid KV ceiling is refused before the Worker starts;
// - lock: a second tab of the same artifact cannot open while the first
//   holds the container ("locked by another writer"), and can afterwards;
// - corrupt: a container that is not format version 1 fails every open;
// - full: past the origin's quota a commit fails and applies nothing;
// - aborted: a run aborted between commits leaves a consistent container;
// - nested: a database below a directory of its root is refused by name
//   (the container has no directories), where native, without a
//   host-created directory, fails to open it too;
// - deep: values just below, at and above the shared KV depth limit (and
//   far above it) store, read back or fail by name equal to native, within
//   the engine's own stack;
// - dag: values with a part shared at two depths count it at its deepest
//   reach in either property order (whatever stores reads back), and a
//   cycle fails by name, equal to native.
//
//   node scripts/test-browser-kv.mjs <varta-executable> [--playwright <module> <firefox|webkit>]
//
// Real local Chrome by default (VARTA_CHROME_BIN overrides the path).
import {mkdtemp, mkdir, writeFile, rm, readFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve, sep} from 'node:path';
import {spawn, spawnSync} from 'node:child_process';
import {createServer} from 'node:http';
import {createHash} from 'node:crypto';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
import {registry} from '../crates/varta-common/browser/registry.mjs';
import {CONTAINER} from '../crates/varta-project/src/browser/kv-store.mjs';

const PROFILE = 'checked-f64-utf16-v38';
const HOST_ABI = registry.profiles.find((profile) => profile.name === PROFILE).hostAbi;
const K = registry.kv;
const ROOT = 'data';
const PATH = `/${ROOT}/app.kv`;
const LOG_PREFIX = 'varta log: ';
const OUT = 'dist/app';
// How long the lock holder keeps the container once it logged holding it:
// well inside the run's deadline.
const HOLD_MS = 800;
// The full case: the origin's quota is its current usage plus a margin
// that the large values (one per commit and timer turn) outgrow.
const QUOTA_MARGIN_BYTES = 2 * 1024 * 1024, LARGE_VALUE_UNITS = 60000, LARGE_VALUES = 64;
// The abort of the aborted case, after some commits (one per timer turn)
// have reached storage.
const ABORT_AFTER_MS = 300, ABORT_COMMITS = 100000;
const STARTUP_POLLS = 500, STARTUP_POLL_MS = 30;
// The longest single DevTools call, a whole browser run included.
const CDP_TIMEOUT_MS = 60000;

const GRANTS = {kv: {read: [`/${ROOT}/*.kv`], write: [`/${ROOT}/*.kv`]}};
const BASIC = `interface Run { id: string; done: boolean; n: number }
export async function main(): Promise<void> {
  const kv = await varta.kv.open(${JSON.stringify(PATH)});
  const first = await kv.atomic().check({ key: ['runs', 'a'], versionstamp: null })
    .set(['runs', 'a'], { id: 'a', done: false, n: 1 }).commit();
  console.log('first ' + String(first.ok));
  const again = await kv.atomic().check({ key: ['runs', 'a'], versionstamp: null })
    .set(['runs', 'a'], { id: 'a', done: true, n: 2 }).commit();
  console.log('again ' + String(again.ok));
  const b = await kv.set(['runs', 'b'], { id: 'b', done: true, n: -0.5 });
  const total = await kv.atomic().sum(['count'], 5n).max(['peak'], 7n).min(['low'], 3n).commit();
  console.log('sum ' + String(total.ok) + ' ' + String(b.versionstamp.length));
  for await (const item of kv.list<Run>({ prefix: ['runs'] })) {
    console.log(item.key.join('/') + ' ' + item.value.id + ' ' + String(item.value.done) + ' ' + String(item.value.n));
  }
  const entry = await kv.get<Run>(['runs', 'a']);
  console.log(entry.value === null ? 'none' : 'a ' + String(entry.value.n) + ' ' + String(entry.versionstamp !== null));
  const count = (await kv.get<KvU64>(['count'])).value;
  console.log(count === null ? 'none' : 'count ' + count.toString());
  try {
    await kv.get<string>(['runs', 'a']);
  } catch (error) {
    if (error instanceof Error) { console.log(error.name + ': ' + error.message); }
  }
  await kv.delete(['runs', 'b']);
  const gone = await kv.get<Run>(['runs', 'b']);
  console.log(gone.value === null ? 'deleted' : 'present');
  kv.close();
}
`;
const GUARDED = (body) => `export async function main(): Promise<void> {
  try {
${body}
  } catch (error) {
    if (error instanceof Error) { console.log(error.name + ': ' + error.message); }
  }
}
`;
const READER = GUARDED(`    const kv = await varta.kv.open(${JSON.stringify(PATH)});
    const entry = await kv.get<string>(['note']);
    console.log(entry.value ?? 'none');
    kv.close();`);
const HOLDER = GUARDED(`    const kv = await varta.kv.open(${JSON.stringify(PATH)});
    await kv.set(['note'], 'held');
    console.log('holding');
    await new Promise<void>((done) => { setTimeout(() => done(), ${HOLD_MS}); });
    kv.close();
    console.log('released');`);
// Reads whether a big value is stored, then stores a small and a big one.
const LARGE = GUARDED(`    const kv = await varta.kv.open(${JSON.stringify(PATH)});
    const count = (await kv.get<KvU64>(['count'])).value;
    if (count !== null) {
      const next = await kv.get<string>(['big', count.toString()]);
      console.log('count ' + count.toString() + ' ' + (next.value === null ? 'absent' : 'present'));
      return;
    }
    for (let i = 0; i < ${LARGE_VALUES}; i++) {
      try {
        await kv.atomic().set(['big', String(i)], 'x'.repeat(${LARGE_VALUE_UNITS})).sum(['count'], 1n).commit();
      } catch (error) {
        if (error instanceof Error) { console.log('at ' + String(i) + ' ' + error.name + ': ' + error.message); }
        break;
      }
      await new Promise<void>((done) => { setTimeout(() => done(), 0); });
    }
    kv.close();`);
// Checks that two KvU64 counters summed by the same commits agree, then
// commits to both, one commit per timer turn, until aborted.
// A database one directory below the root.
const NESTED_PATH = `/${ROOT}/sub/app.kv`;
const NESTED = GUARDED(`    const kv = await varta.kv.open(${JSON.stringify(NESTED_PATH)});
    await kv.set(['note'], 'nested');
    console.log('set');
    kv.close();`);
// Chains of records just below, at and above the shared depth limit, and
// far above it (a depth that once overflowed the native host stack).
const FAR_DEPTH = 2000;
const DEEP_RECURSIVE = `interface Chain { next?: Chain }
`;
const DEEP = DEEP_RECURSIVE + GUARDED(`    const kv = await varta.kv.open(${JSON.stringify(PATH)});
    for (const depth of [${K.maxValueDepth - 1}, ${K.maxValueDepth}, ${K.maxValueDepth + 1}, ${FAR_DEPTH}]) {
      let value: Chain = {};
      for (let i = 0; i < depth; i++) { value = { next: value }; }
      try { await kv.set(['deep', depth], value); console.log('set ' + String(depth)); }
      catch (error) { if (error instanceof Error) { console.log(error.name + ': ' + error.message); } }
      const got = await kv.get<Chain>(['deep', depth]);
      let n = 0; let at = got.value;
      while (at !== null && at.next !== undefined) { at = at.next; n++; }
      console.log('get ' + String(n));
    }
    kv.close();`);
// Holders { a, b } of a chain x of `inner` records and y, `wraps` records
// around the same x, in both property orders (the native
// shared_parts_keep_the_depth_limit_at_their_deepest_reach program): a
// shared part counts at its deepest reach, so a holder stores exactly when
// its expansion fits, and whatever stores reads back whole. Then a cycle.
const SHARED_WRAPS = 10, SHARED_REPORTED = [SHARED_WRAPS, 60];
const DAG_CASES = [[0, K.maxValueDepth - 1], [0, K.maxValueDepth], [SHARED_WRAPS, K.maxValueDepth - 1 - SHARED_WRAPS],
  [SHARED_WRAPS, K.maxValueDepth - SHARED_WRAPS], SHARED_REPORTED];
const DAG = `interface Chain { next?: Chain }
interface Holder { a: Chain; b: Chain }
function wrap(inner: Chain, levels: number): Chain {
  let value: Chain = inner;
  for (let i = 0; i < levels; i++) { value = { next: value }; }
  return value;
}
function length(chain: Chain): number {
  let n = 0; let at: Chain | null = chain;
  while (at !== null && at.next !== undefined) { at = at.next; n++; }
  return n;
}
` + GUARDED(`    const kv = await varta.kv.open(${JSON.stringify(PATH)});
    const wrapsOf: number[] = [${DAG_CASES.map(([wraps]) => wraps).join(', ')}];
    const innerOf: number[] = [${DAG_CASES.map(([, inner]) => inner).join(', ')}];
    for (let i = 0; i < wrapsOf.length; i++) {
      const wraps = wrapsOf[i]; const inner = innerOf[i];
      for (const xFirst of [true, false]) {
        const x = wrap({}, inner); const y = wrap(x, wraps);
        const holder: Holder = xFirst ? { a: x, b: y } : { a: y, b: x };
        const label = String(wraps) + ' ' + String(inner) + (xFirst ? ' x' : ' y');
        try { await kv.set(['dag', label], holder); console.log('set ' + label); }
        catch (e) { if (e instanceof Error) { console.log(label + ' ' + e.name + ': ' + e.message); } }
        try {
          const got = await kv.get<Holder>(['dag', label]);
          if (got.value === null) { console.log('get ' + label + ' absent'); }
          else { console.log('get ' + label + ' ' + String(length(got.value.a)) + ' ' + String(length(got.value.b))); }
        } catch (e) { if (e instanceof Error) { console.log('get ' + label + ' ' + e.name + ': ' + e.message); } }
      }
    }
    const c: Chain = {}; const d: Chain = { next: c }; c.next = d;
    try { await kv.set(['dag', 'cycle'], { a: c, b: d }); console.log('set cycle'); }
    catch (e) { if (e instanceof Error) { console.log('cycle ' + e.name + ': ' + e.message); } }
    kv.close();`);
/** What DAG logs: the holder's deepest reach is 1 + wraps + inner. */
const dagLogs = () => [...DAG_CASES.flatMap(([wraps, inner]) => [true, false].flatMap((xFirst) => {
  const label = `${wraps} ${inner} ${xFirst ? 'x' : 'y'}`;
  if (1 + wraps + inner > K.maxValueDepth) return [`${label} ${K.errors.typeError}: ${K.valueTooDeep}`, `get ${label} absent`];
  const [x, y] = [inner, wraps + inner];
  return [`set ${label}`, `get ${label} ${xFirst ? x : y} ${xFirst ? y : x}`];
})), `cycle ${K.errors.typeError}: a cyclic value cannot be stored in a KV value`].map((line) => LOG_PREFIX + line);
const COUNTER = GUARDED(`    const kv = await varta.kv.open(${JSON.stringify(PATH)});
    const count = (await kv.get<KvU64>(['count'])).value;
    const twin = (await kv.get<KvU64>(['twin'])).value;
    console.log('consistent ' + String(count === null ? twin === null : twin !== null && count.toString() === twin.toString()));
    for (let i = 0; i < ${ABORT_COMMITS}; i++) {
      await kv.atomic().sum(['count'], 1n).sum(['twin'], 1n).commit();
      await new Promise<void>((done) => { setTimeout(() => done(), 0); });
    }
    kv.close();`);

async function project(root, name, source, capabilities, policy) {
  const directory = join(root, name);
  await mkdir(join(directory, ROOT), {recursive: true});
  await writeFile(join(directory, 'main.ts'), source);
  await writeFile(join(directory, 'varta.config.json'), JSON.stringify({entry: 'main.ts', target: 'wasm', out: OUT, languageProfile: PROFILE,
    fs: {roots: {[ROOT]: ROOT}}, capabilities}));
  if (policy) await writeFile(join(directory, 'host-policy.json'), JSON.stringify({schemaVersion: 1, abiVersion: HOST_ABI, fsRoots: {[ROOT]: join(directory, ROOT)}, capabilities: policy}));
  return directory;
}

const number = Number;
function nativeRun(varta, directory, policy) {
  const args = [...(policy ? ['--policy', 'host-policy.json'] : []), 'dev', '--budget-report'];
  const run = spawnSync(varta, args, {cwd: directory, encoding: 'utf8'});
  if (run.error) throw run.error;
  assert.equal(run.status, 0, run.stderr);
  const turns = [...run.stderr.matchAll(/varta budget: turn \d+ \w+: fuel (\d+)\/\d+, host calls (\d+)\/\d+, microtasks (\d+),/g)]
    .map(([, fuel, hostCalls, microtasks]) => ({fuel: number(fuel), hostCalls: number(hostCalls), microtasks: number(microtasks)}));
  const total = /varta budget: total: (\d+) turns, fuel (\d+), peak memory \d+\/\d+ bytes, output (\d+)\/\d+ bytes/.exec(run.stderr);
  assert(total, run.stderr);
  if (/later turns not listed/.test(run.stderr)) throw new Error('native report truncated');
  return {
    logs: run.stdout.split(/\r?\n/).filter((line) => line.startsWith(LOG_PREFIX)),
    usage: {turns: number(total[1]), fuelUsed: number(total[2]), outputBytes: number(total[3]),
      hostCalls: turns.reduce((sum, turn) => sum + turn.hostCalls, 0), microtasks: turns.reduce((sum, turn) => sum + turn.microtasks, 0)},
  };
}

/** Build the browser bundle of `directory`; a KV grant builds. */
function build(varta, directory) {
  const run = spawnSync(varta, ['build', '--target', 'browser'], {cwd: directory, encoding: 'utf8'});
  if (run.error) throw run.error;
  assert.equal(run.status, 0, run.stderr);
  // The bundle directory the build reports writing its page to.
  const page = /wrote artifact: (.+)\/index\.html/.exec(run.stdout);
  assert(page, run.stdout);
  return resolve(directory, page[1]);
}

/** The OPFS directory of the artifact a bundle runs (`sha256-<digest>`). */
async function artifactDirectory(bundle) {
  return CONTAINER.artifactPrefix + createHash('sha256').update(await readFile(join(bundle, 'app.wasm'))).digest('hex');
}

/** One loopback origin serving every bundle under /<case>/ and a blank page
 * per case, so the loader runs only when a test calls it. */
async function serveBundles(bundles) {
  const server = createServer(async (request, response) => {
    try {
      const [, name, ...rest] = decodeURIComponent(new URL(request.url, 'http://loopback').pathname).split('/');
      const base = bundles.get(name);
      if (!base) throw new Error('unknown case');
      const file = rest.join('/');
      if (file === 'blank.html') { response.setHeader('Content-Type', 'text/html'); response.end('<!doctype html><meta charset="utf-8"><title>kv</title>'); return; }
      const path = resolve(base, file);
      if (!path.startsWith(resolve(base) + sep)) throw new Error('outside the bundle');
      const bytes = await readFile(path);
      response.setHeader('Cache-Control', 'no-store');
      response.setHeader('Content-Type', path.endsWith('.js') ? 'text/javascript' : path.endsWith('.wasm') ? 'application/wasm' : path.endsWith('.json') ? 'application/json' : 'application/octet-stream');
      response.end(bytes);
    } catch { response.writeHead(404).end(); }
  });
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  return {server, origin: `http://127.0.0.1:${server.address().port}`};
}

// One browser run of the case's loader; the expression is self-contained.
const runExpression = (name, options = '{}') => `(async()=>{const {run}=await import('/${name}/varta-host.js');window.__kvAbort=new AbortController();window.__kvLogs=[];try{const r=await run({...${options},signal:window.__kvAbort.signal,onLog:(line)=>window.__kvLogs.push(line)});return {status:r.status,error:r.error,logs:r.logs,usage:{turns:r.turns,fuelUsed:r.fuelUsed,outputBytes:r.outputBytes,hostCalls:r.hostCalls,microtasks:r.microtasks},audit:r.permissions.audit.map(a=>[a.operation,a.resource,a.outcome])};}catch(error){return {status:'thrown',error:String(error?.message??error),logs:error?.logs??[]};}})()`;
// Replace the artifact's container with `bytes` from the page (OPFS is
// reachable from the page through a writable stream).
const corruptExpression = (directory, bytes) => `(async()=>{const root=await navigator.storage.getDirectory();const area=await root.getDirectoryHandle(${JSON.stringify(CONTAINER.directory)},{create:true});const dir=await area.getDirectoryHandle(${JSON.stringify(directory)},{create:true});const file=await dir.getFileHandle(${JSON.stringify(CONTAINER.file)},{create:true});const w=await file.createWritable();await w.write(new Uint8Array(${JSON.stringify(bytes)}));await w.close();return true;})()`;

/** Wait until `expression` holds in `tab`. */
async function until(tab, expression) {
  for (let i = 0; !(await tab.evaluate(expression)); i++) {
    if (i > STARTUP_POLLS) throw new Error(`timed out waiting for ${expression}`);
    await new Promise((done) => setTimeout(done, STARTUP_POLL_MS));
  }
}
const denial = (id, path) => K.denialText.replace(K.capabilityPlaceholder, () => id).replace(K.pathPlaceholder, () => path);
const text = (name, path, failure = '') => K.texts[name].replace(K.pathPlaceholder, () => path).replace(K.failurePlaceholder, () => failure);

/** Run every case; `driver.page(url)` opens a tab of one browser profile
 * (`evaluate`, `reload`, `close` and, where the engine can, `quota(origin,
 * bytes)`, which overrides the origin's storage quota; undefined restores
 * it). */
export async function runKv(varta, driver) {
  if (typeof driver === 'function') throw new Error('runKv needs a multi-tab driver: run node scripts/test-browser-kv.mjs <varta> [--playwright <module> <engine>]');
  const root = await mkdtemp(join(tmpdir(), 'varta-browser-kv-'));
  const passed = [];
  const pass = (...names) => { passed.push(...names); console.log(`PASS ${names.join(', ')}`); };
  const directories = new Map(), bundles = new Map();
  const add = async (name, source, capabilities, policy) => {
    const directory = await project(root, name, source, capabilities, policy);
    directories.set(name, directory); bundles.set(name, build(varta, directory));
    return directory;
  };
  try {
    await add('basic', BASIC, GRANTS);
    await add('isolated', READER, GRANTS);
    await add('denied', READER, {});
    await add('ceiling', READER, GRANTS, [{id: K.capabilities.read, patterns: [`/${ROOT}/other.kv`]}, {id: K.capabilities.write, patterns: [`/${ROOT}/other.kv`]}]);
    await add('lock', HOLDER, {...GRANTS, time: {timer: true}});
    await add('corrupt', READER, GRANTS);
    await add('full', LARGE, {...GRANTS, time: {timer: true}});
    await add('aborted', COUNTER, {...GRANTS, time: {timer: true}});
    await add('nested', NESTED, {kv: {read: [`/${ROOT}/**`], write: [`/${ROOT}/**`]}});
    await add('deep', DEEP, GRANTS);
    await add('dag', DAG, GRANTS);
    const {server, origin} = await serveBundles(bundles);
    const page = async (name) => driver.page(`${origin}/${name}/blank.html`);
    const same = (label, browser, native) => {
      assert.equal(browser.status, 'ok', `${label}: ${browser.error}`);
      assert.deepEqual(browser.logs, native.logs, `${label}: logs`);
      assert.deepEqual(browser.usage, native.usage, `${label}: usage`);
    };
    try {
      // Basic, then persistence across a reload: native runs twice over its file.
      {
        const native = [nativeRun(varta, directories.get('basic')), nativeRun(varta, directories.get('basic'))];
        assert.deepEqual(native[0].logs.slice(0, 2), [`${LOG_PREFIX}first true`, `${LOG_PREFIX}again false`], 'native basic');
        assert.deepEqual(native[1].logs.slice(0, 2), [`${LOG_PREFIX}first false`, `${LOG_PREFIX}again false`], 'native second run');
        const tab = await page('basic');
        same('basic', await tab.evaluate(runExpression('basic')), native[0]);
        await tab.reload();
        same('reload', await tab.evaluate(runExpression('basic')), native[1]);
        await tab.close();
        pass('basic', 'persistence-across-reload');
      }
      // Isolation: another artifact on the same origin starts empty.
      {
        const native = nativeRun(varta, directories.get('isolated'));
        assert.deepEqual(native.logs, [`${LOG_PREFIX}none`]);
        const tab = await page('isolated');
        same('isolation', await tab.evaluate(runExpression('isolated')), native);
        await tab.close();
        pass('isolation');
      }
      // No grant: every open is denied and audited.
      {
        const native = nativeRun(varta, directories.get('denied'));
        const message = denial(K.capabilities.read, PATH);
        assert.deepEqual(native.logs, [`${LOG_PREFIX}${message}`, `${LOG_PREFIX}${K.notAllowed}: ${message}`]);
        const tab = await page('denied');
        const browser = await tab.evaluate(runExpression('denied'));
        same('missing grant', browser, native);
        assert.deepEqual(browser.audit.filter(([, resource]) => resource !== ''), [[K.capabilities.read, PATH, 'denied']]);
        await tab.close();
        pass('missing-grant');
      }
      // A narrower host ceiling denies as native's host policy does; an
      // invalid KV ceiling is refused before the Worker starts.
      {
        const directory = directories.get('ceiling');
        const native = nativeRun(varta, directory, true);
        const ceiling = JSON.stringify({[K.capabilities.read]: {patterns: [`/${ROOT}/other.kv`]}, [K.capabilities.write]: {patterns: [`/${ROOT}/other.kv`]}});
        const tab = await page('ceiling');
        same('host ceiling', await tab.evaluate(runExpression('ceiling', `{permissionCeiling:${ceiling}}`)), native);
        for (const patterns of [['data/*.kv'], ['/data/../x.kv'], ['**'], ['   '], [`/${ROOT}/${'x'.repeat(4096)}`]]) {
          const refused = await tab.evaluate(runExpression('ceiling', `{permissionCeiling:${JSON.stringify({[K.capabilities.read]: {patterns}})}}`));
          assert.equal(refused.status, 'thrown', JSON.stringify(patterns));
          assert.equal(refused.error, 'invalid KV permission ceiling', JSON.stringify(patterns));
        }
        await tab.close();
        pass('host-ceiling');
      }
      // Two tabs of one artifact: the second is locked out while the first
      // holds the container, and opens once it is released.
      {
        const first = await page('lock');
        const holding = first.evaluate(runExpression('lock'));
        await until(first, `window.__kvLogs?.includes(${JSON.stringify(`${LOG_PREFIX}holding`)})`);
        const second = await page('lock');
        const locked = await second.evaluate(runExpression('lock'));
        assert.equal(locked.status, 'ok', locked.error);
        assert.deepEqual(locked.logs, [`${LOG_PREFIX}${K.errors.typeError}: ${text('locked', PATH)}`]);
        const held = await holding;
        assert.deepEqual(held.logs, [`${LOG_PREFIX}holding`, `${LOG_PREFIX}released`], held.error);
        const after = await second.evaluate(runExpression('lock'));
        assert.deepEqual(after.logs, [`${LOG_PREFIX}holding`, `${LOG_PREFIX}released`], after.error);
        await first.close(); await second.close();
        pass('two-tabs-lock');
      }
      // A corrupt container fails every open with the corruption named.
      {
        const tab = await page('corrupt');
        await tab.evaluate(corruptExpression(await artifactDirectory(bundles.get('corrupt')), new Array(256).fill(0x5a)));
        const browser = await tab.evaluate(runExpression('corrupt'));
        assert.equal(browser.status, 'ok', browser.error);
        assert.deepEqual(browser.logs, [`${LOG_PREFIX}${K.errors.typeError}: ${text('corrupt', PATH, 'no valid header')}`]);
        await tab.close();
        pass('corrupt');
      }
      // Past the quota a commit fails and applies nothing: with the quota
      // restored the next run finds exactly the values committed before it
      // and stores the rest.
      const quotaTab = await page('full');
      if (quotaTab.quota) {
        const tab = quotaTab;
        const usage = await tab.evaluate('navigator.storage.estimate().then((estimate) => estimate.usage)');
        await tab.quota(origin, usage + QUOTA_MARGIN_BYTES);
        const full = await tab.evaluate(runExpression('full'));
        await tab.quota(origin, undefined);
        assert.equal(full.status, 'ok', full.error);
        const failure = new RegExp(`^${LOG_PREFIX}at (\\d+) ${K.errors.typeError}: `).exec(full.logs[0] ?? '');
        assert(failure, JSON.stringify(full.logs));
        assert.equal(full.logs[0].slice(failure[0].length), text('full', PATH, K.quotaError));
        // The failed commit's value and count are absent.
        const again = await tab.evaluate(runExpression('full'));
        assert.deepEqual(again.logs, [`${LOG_PREFIX}count ${failure[1]} absent`], again.error);
        await tab.close();
        pass('full');
      } else { await quotaTab.close(); console.log('SKIP full: this driver cannot set a storage quota'); }
      // A run aborted between commits leaves a consistent container.
      {
        const tab = await page('aborted');
        const running = tab.evaluate(runExpression('aborted'));
        await new Promise((done) => setTimeout(done, ABORT_AFTER_MS));
        await tab.evaluate('window.__kvAbort.abort()');
        const aborted = await running;
        assert.equal(aborted.status, 'thrown', JSON.stringify(aborted));
        assert.deepEqual(aborted.logs, [`${LOG_PREFIX}consistent true`]);
        await tab.reload();
        const again = tab.evaluate(runExpression('aborted'));
        await new Promise((done) => setTimeout(done, ABORT_AFTER_MS));
        await tab.evaluate('window.__kvAbort.abort()');
        assert.deepEqual((await again).logs, [`${LOG_PREFIX}consistent true`]);
        await tab.close();
        pass('aborted');
      }
      // A nested database path: the browser refuses it by name; native,
      // without a host-created directory, cannot open it either.
      {
        const native = nativeRun(varta, directories.get('nested'));
        const opening = text('unavailable', NESTED_PATH, '');
        assert.equal(native.logs.length, 1, JSON.stringify(native.logs));
        assert(native.logs[0].startsWith(`${LOG_PREFIX}${K.errors.typeError}: ${opening}`), native.logs[0]);
        const tab = await page('nested');
        const browser = await tab.evaluate(runExpression('nested'));
        assert.equal(browser.status, 'ok', browser.error);
        assert.deepEqual(browser.logs, [`${LOG_PREFIX}${K.errors.typeError}: ${text('unavailable', NESTED_PATH, K.noDirectories)}`]);
        await tab.close();
        pass('nested');
      }
      // Deep values: equal to native at, just below and above the limit.
      {
        const native = nativeRun(varta, directories.get('deep'));
        const refused = `${LOG_PREFIX}${K.errors.typeError}: ${K.valueTooDeep}`;
        assert.deepEqual(native.logs, [
          ...[K.maxValueDepth - 1, K.maxValueDepth].flatMap((depth) => [`${LOG_PREFIX}set ${depth}`, `${LOG_PREFIX}get ${depth}`]),
          refused, `${LOG_PREFIX}get 0`, refused, `${LOG_PREFIX}get 0`,
        ]);
        const tab = await page('deep');
        same('deep', await tab.evaluate(runExpression('deep')), native);
        await tab.close();
        pass('deep');
      }
      // Shared parts: equal to native, and to the deepest-reach rule.
      {
        const native = nativeRun(varta, directories.get('dag'));
        assert.deepEqual(native.logs, dagLogs());
        const tab = await page('dag');
        same('dag', await tab.evaluate(runExpression('dag')), native);
        await tab.close();
        pass('dag');
      }
    } finally { await new Promise((done) => server.close(done)); }
  } finally { await rm(root, {recursive: true, force: true}); }
  return passed;
}

/** A Chrome DevTools Protocol driver over one headless Chrome profile. */
async function chromeDriver() {
  const profile = await mkdtemp(join(tmpdir(), 'varta-browser-kv-chrome-'));
  const executable = process.env.VARTA_CHROME_BIN ?? (process.platform === 'darwin' ? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome' : 'chromium');
  const child = spawn(executable, ['--headless', '--no-first-run', '--no-default-browser-check', '--disable-background-networking', '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank'], {stdio: 'ignore'});
  let spawnError; child.on('error', (error) => { spawnError = error; });
  let endpoint;
  for (let i = 0; !endpoint; i++) {
    if (spawnError) throw spawnError;
    if (child.exitCode !== null || i > STARTUP_POLLS) throw new Error('Chrome did not start');
    try { const [port, path] = (await readFile(join(profile, 'DevToolsActivePort'), 'utf8')).split('\n'); endpoint = `ws://127.0.0.1:${port}${path}`; }
    catch { await new Promise((done) => setTimeout(done, STARTUP_POLL_MS)); }
  }
  const socket = new WebSocket(endpoint);
  await new Promise((done, fail) => { socket.addEventListener('open', done, {once: true}); socket.addEventListener('error', fail, {once: true}); });
  let next = 0; const waiting = new Map();
  socket.addEventListener('message', ({data}) => {
    const message = JSON.parse(data); const entry = waiting.get(message.id); if (!entry) return;
    waiting.delete(message.id);
    if (message.error) entry.reject(new Error(JSON.stringify(message.error))); else entry.resolve(message.result);
  });
  const call = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++next, timer = setTimeout(() => { waiting.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, CDP_TIMEOUT_MS);
    waiting.set(id, {resolve: (value) => { clearTimeout(timer); resolve(value); }, reject: (error) => { clearTimeout(timer); reject(error); }});
    socket.send(JSON.stringify({id, method, params, ...(sessionId ? {sessionId} : {})}));
  });
  const loaded = async (sessionId, url) => {
    for (let i = 0; ; i++) {
      const {result} = await call('Runtime.evaluate', {expression: `document.readyState==='complete'&&location.href===${JSON.stringify(url)}`, returnByValue: true}, sessionId);
      if (result.value === true) return;
      if (i > STARTUP_POLLS) throw new Error('page did not load');
      await new Promise((done) => setTimeout(done, STARTUP_POLL_MS));
    }
  };
  return {
    async page(url) {
      const {targetId} = await call('Target.createTarget', {url: 'about:blank'});
      const {sessionId} = await call('Target.attachToTarget', {targetId, flatten: true});
      await call('Page.enable', {}, sessionId); await call('Runtime.enable', {}, sessionId);
      // A visible page: a background page's timers are throttled.
      await call('Emulation.setFocusEmulationEnabled', {enabled: true}, sessionId);
      await call('Page.navigate', {url}, sessionId); await loaded(sessionId, url);
      return {
        async evaluate(expression) {
          const response = await call('Runtime.evaluate', {expression, awaitPromise: true, returnByValue: true}, sessionId);
          if (response.exceptionDetails) throw new Error(JSON.stringify(response.exceptionDetails));
          return response.result.value;
        },
        async reload() { await call('Page.reload', {}, sessionId); await new Promise((done) => setTimeout(done, STARTUP_POLL_MS)); await loaded(sessionId, url); },
        async close() { await call('Target.closeTarget', {targetId}); },
        async quota(origin, bytes) {
          await call('Storage.overrideQuotaForOrigin', {origin, ...(bytes === undefined ? {} : {quotaSize: bytes})}, sessionId);
        },
      };
    },
    async close() {
      socket.close();
      if (child.exitCode === null && child.signalCode === null) {
        child.kill('SIGTERM');
        await Promise.race([new Promise((done) => child.once('exit', done)), new Promise((done) => setTimeout(done, 3000))]);
      }
      if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
      await rm(profile, {recursive: true, force: true});
    },
  };
}

// The Playwright engine whose DevTools session can override a quota.
const CHROMIUM = 'chromium';
/** A Playwright driver: one persistent browser context (OPFS needs one in
 * WebKit) of the Playwright browser type `type`, launched with `options`.
 * Chromium also overrides the storage quota through its DevTools session.
 * scripts/test-browser-matrix.mjs runs the KV group through it. */
export async function playwrightKvDriver(type, options = {}) {
  const engine = type.name();
  const directory = await mkdtemp(join(tmpdir(), `varta-browser-kv-${engine}-`));
  const context = await type.launchPersistentContext(directory, {headless: true, ...options});
  return {
    async page(url) {
      const tab = await context.newPage();
      await tab.goto(url);
      const session = engine === CHROMIUM ? await context.newCDPSession(tab) : undefined;
      return {
        evaluate: (expression) => tab.evaluate(expression), reload: () => tab.reload(), close: () => tab.close(),
        ...(session ? {quota: (origin, bytes) => session.send('Storage.overrideQuotaForOrigin', {origin, ...(bytes === undefined ? {} : {quotaSize: bytes})})} : {}),
      };
    },
    async close() { await context.close(); await rm(directory, {recursive: true, force: true}); },
  };
}
async function playwrightDriver(module, engine) {
  const playwright = await import(pathToFileURL(resolve(module)).href);
  return playwrightKvDriver((playwright.default ?? playwright)[engine]);
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  const [executable, flag, module, engine] = process.argv.slice(2);
  if (!executable || flag !== undefined && (flag !== '--playwright' || !module || !['firefox', 'webkit', 'chromium'].includes(engine))) {
    throw new Error('Usage: node scripts/test-browser-kv.mjs <varta-executable> [--playwright <module> <firefox|webkit|chromium>]');
  }
  const driver = flag ? await playwrightDriver(module, engine) : await chromeDriver();
  try {
    const passed = await runKv(resolve(executable), driver);
    console.log(`${flag ? engine : 'Chrome'} KV acceptance passed (${passed.join(', ')}): OPFS containers with native logs, exact fuel, host calls, microtasks, output bytes and turns`);
  } finally { await driver.close(); }
}
