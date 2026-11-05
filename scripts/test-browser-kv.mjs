// Native/browser equivalence of v38 KV storage in the official browser host,
// which offers no KV storage yet and implements the contract of a host
// without it (docs/embedding.md, KV storage (v38)): every `kv_open` is
// denied exactly like an open no grant covers.
//
// One program opens two databases, commits a checked write and reads it
// back, logging what rejects. It runs natively (`varta dev --budget-report`)
// and in the browser host served by `varta dev --target browser`:
//
// - no grant: the native and browser runs are equal (logs, exact fuel,
//   host calls, microtasks, output bytes and turns), and the browser audits
//   each denied `kv.read`;
// - granted, with an empty host ceiling: native denies through the ceiling
//   and the browser through `permissionCeiling: {}`; the runs are equal;
// - granted and mapped: native opens, commits and reads the database; the
//   browser still denies, and its run equals the native run without a grant,
//   while the broker records the grant's initial state and the denied
//   operations. Native success here is the documented gap (no browser KV
//   storage), not an equivalence.
//
//   node scripts/test-browser-kv.mjs <varta-executable>
//
// Real local Chrome (VARTA_CHROME_BIN overrides the path). Other engines run
// the same cases through scripts/test-browser-matrix.mjs --kv.
import {mkdtemp, mkdir, writeFile, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawn, spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
import {registry} from '../crates/varta-common/browser/registry.mjs';

const PROFILE = 'checked-f64-utf16-v38';
const HOST_ABI = registry.profiles.find((profile) => profile.name === PROFILE).hostAbi;
const K = registry.kv;
const ROOT = 'data';
// The second path keeps a `$&` replacement pattern literal in the denial.
const PATHS = [`/${ROOT}/app.kv`, `/${ROOT}/$&.kv`];
const STARTUP_POLLS = 500, STARTUP_POLL_MS = 30;
const LOG_PREFIX = 'varta log: ';

const SOURCE = `export async function main(): Promise<void> {
  for (const path of ${JSON.stringify(PATHS)}) {
    try {
      const kv = await varta.kv.open(path);
      const result = await kv.atomic()
        .check({ key: ['credentials', 'apiKey'], versionstamp: null })
        .set(['credentials', 'apiKey'], 'secret')
        .commit();
      const entry = await kv.get<string>(['credentials', 'apiKey']);
      console.log(path + ' ' + String(result.ok) + ' ' + (entry.value ?? 'none'));
      kv.close();
    } catch (error) {
      if (error instanceof Error) { console.log(path + ' ' + error.name + ': ' + error.message); }
    }
  }
}
`;
const GRANTS = {kv: {read: [`/${ROOT}/*.kv`], write: [`/${ROOT}/*.kv`]}};

const CASES = [
  {name: 'no-grant', capabilities: {}},
  {name: 'empty-ceiling', capabilities: GRANTS, policy: [], ceiling: {}},
  {name: 'granted', capabilities: GRANTS, nativeOpens: true},
];

async function project(root, item) {
  const directory = join(root, item.name);
  await mkdir(join(directory, ROOT), {recursive: true});
  await writeFile(join(directory, 'main.ts'), SOURCE);
  await writeFile(join(directory, 'varta.config.json'), JSON.stringify({entry: 'main.ts', target: 'wasm', out: 'dist/app', languageProfile: PROFILE,
    fs: {roots: {[ROOT]: ROOT}}, capabilities: item.capabilities}));
  if (item.policy) await writeFile(join(directory, 'host-policy.json'), JSON.stringify({schemaVersion: 1, abiVersion: HOST_ABI, fsRoots: {[ROOT]: join(directory, ROOT)}, capabilities: item.policy}));
  return directory;
}

const number = Number;
function nativeRun(varta, directory, item) {
  const args = [...(item.policy ? ['--policy', 'host-policy.json'] : []), 'dev', '--budget-report'];
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

async function serve(varta, directory) {
  const child = spawn(varta, ['dev', '--target', 'browser'], {cwd: directory, stdio: ['ignore', 'pipe', 'pipe']});
  let stdout = '', stderr = '';
  child.stdout.on('data', (chunk) => { stdout += chunk; });
  child.stderr.on('data', (chunk) => { stderr += chunk; });
  for (let i = 0; ; i++) {
    const url = /varta dev: browser (http:\/\/[^\s]+)/.exec(stdout)?.[1];
    if (url) return {url, child};
    if (child.exitCode !== null) throw new Error(stderr);
    if (i > STARTUP_POLLS) throw new Error('browser dev startup timeout');
    await new Promise((done) => setTimeout(done, STARTUP_POLL_MS));
  }
}
async function stop(child) {
  if (child.exitCode === null && child.signalCode === null) {
    const exited = new Promise((done) => child.once('exit', done));
    child.kill('SIGINT');
    await exited;
  }
}

// One browser execution; the expression is self-contained so that both the
// CDP and the Playwright drivers evaluate it unchanged.
function expression(item) {
  const options = JSON.stringify(item.ceiling ? {permissionCeiling: item.ceiling} : {});
  return `(async()=>{const {run}=await import(document.querySelector('script[data-varta-dev]').src);const r=await run(${options});return {status:r.status,error:r.error,logs:r.logs,usage:{turns:r.turns,fuelUsed:r.fuelUsed,outputBytes:r.outputBytes,hostCalls:r.hostCalls,microtasks:r.microtasks},audit:r.permissions.audit.map(a=>[a.operation,a.resource,a.outcome])};})()`;
}

const denial = (path) => K.denialText.replace(K.pathPlaceholder, () => path);
const deniedLogs = PATHS.flatMap((path) => [`${LOG_PREFIX}${denial(path)}`, `${LOG_PREFIX}${path} ${K.notAllowed}: ${denial(path)}`]);
const deniedAudit = PATHS.map((path) => [K.capabilities.read, path, 'denied']);

/** Run every case; `open(url, evaluate => …)` drives one browser page. */
export async function runKv(varta, open) {
  const root = await mkdtemp(join(tmpdir(), 'varta-browser-kv-'));
  const results = new Map();
  try {
    for (const item of CASES) {
      const directory = await project(root, item);
      const native = nativeRun(varta, directory, item);
      const {url, child} = await serve(varta, directory);
      let browser;
      try { browser = await open(url, (evaluate) => evaluate(expression(item))); } finally { await stop(child); }
      results.set(item.name, {native, browser});
      const label = (what) => `${item.name}: ${what}`;
      assert.equal(browser.status, 'ok', label(browser.error));
      assert.deepEqual(browser.logs, deniedLogs, label('browser logs'));
      // Only operation records: grant states are recorded before them.
      assert.deepEqual(browser.audit.filter(([, resource]) => resource !== ''), deniedAudit, label('browser audit'));
      if (item.nativeOpens) {
        assert.deepEqual(native.logs, PATHS.map((path) => `${LOG_PREFIX}${path} true secret`), label('native opens the granted database'));
        assert.deepEqual(browser.audit.filter(([, resource]) => resource === '').map(([id, , outcome]) => [id, outcome]),
          [[K.capabilities.read, 'granted'], [K.capabilities.write, 'granted']], label('granted initial states'));
      } else {
        assert.deepEqual(browser.logs, native.logs, label('logs equal native'));
        assert.deepEqual(browser.usage, native.usage, label('usage equals native'));
      }
    }
    // Granted, the browser run is the run of a host without a grant.
    const granted = results.get('granted').browser, noGrant = results.get('no-grant').native;
    assert.deepEqual(granted.logs, noGrant.logs, 'granted browser logs equal the native run without a grant');
    assert.deepEqual(granted.usage, noGrant.usage, 'granted browser usage equals the native run without a grant');
  } finally { await rm(root, {recursive: true, force: true}); }
  return CASES.map(({name}) => name);
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  if (process.argv.length !== 3) throw new Error('Usage: node scripts/test-browser-kv.mjs <varta-executable>');
  const {withBrowserPage} = await import('./browser-probe.mjs');
  const passed = await runKv(resolve(process.argv[2]), (url, action) => withBrowserPage(url, (call) => action(async (text) => {
    const response = await call('Runtime.evaluate', {expression: text, awaitPromise: true, returnByValue: true});
    if (response.exceptionDetails) throw new Error(JSON.stringify(response.exceptionDetails));
    return response.result.value;
  })));
  console.log(`Chrome KV acceptance passed (${passed.join(', ')}): every open is denied as a host without KV storage must deny it, with native logs, exact fuel, host calls, microtasks, output bytes and turns, and audited kv.read denials`);
}
