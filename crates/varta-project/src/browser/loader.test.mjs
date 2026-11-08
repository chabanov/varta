// The loader's idle-collection watchdog and mount readiness, against a fake
// Worker. The loader module is assembled as the bundle assembles it: the
// generated descriptor, the loader's registry subset, the network policy and
// the loader, with the engine modules it names imported as namespaces.
import {test, after} from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdtemp, readFile, rm, writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {pathToFileURL} from 'node:url';
import {registry} from '../../../varta-common/browser/registry.mjs';

const C = registry.effects.collector;
// Test budgets: a per-turn deadline no test reaches and a short idle period.
const TURN_DEADLINE_MS = 10000;
const IDLE_DEADLINE_MS = 20;
// Waiting this many idle deadlines shows a cleared watchdog never fires.
const CLEARED_WAIT_PERIODS = 5;
const OUTPUT_BYTES = 1 << 20, MEMORY_BYTES = 1 << 24;
const EXIT_CODE = 5;
const MODULES = {
  sessionChannel: 'session-channel.mjs',
  environment: 'environment.mjs', permissionBroker: 'permission-broker.mjs', permissionStore: 'permission-store.mjs',
  permissionPrompt: 'permission-prompt.mjs', eventSources: 'event-sources.mjs', viewCodec: 'view-codec.mjs',
  viewQueue: 'view-queue.mjs', viewFonts: 'view-fonts.mjs', viewPolicy: 'view-policy.mjs', viewModel: 'view-model.mjs',
  viewDom: 'view-dom.mjs', uiTiming: 'ui-timing.mjs',
};

const identity = bytes => 'sha256:' + createHash('sha256').update(bytes).digest('hex');
const files = {
  'app.wasm': new Uint8Array([0]), 'app.private.wasm': new Uint8Array([1]),
  'app.manifest.json': new TextEncoder().encode(JSON.stringify({capabilities: []})),
  'varta-worker.js': new TextEncoder().encode('// fake worker'),
};
const item = name => ({name, bytes: files[name].byteLength, identity: identity(files[name])});
const rule = (production, maximum = production) => ({production, maximum});
const budgets = {deadlinePerTurnMs: TURN_DEADLINE_MS, outputBytes: OUTPUT_BYTES, memoryBytes: MEMORY_BYTES, [C.deadlineBudget]: TURN_DEADLINE_MS};
const descriptor = {
  schemaVersion: 1, profile: 'checked-f64-utf16-v38', trustedTypesPolicy: 'varta-worker', hostModule: 'varta',
  original: item('app.wasm'), image: item('app.private.wasm'), manifest: item('app.manifest.json'), worker: item('varta-worker.js'),
  booleanCapabilities: registry.booleanCapabilities, network: registry.network, view: registry.view, hasView: false,
  permissions: registry.permissions, environment: registry.environment, effects: registry.effects, hasPermissions: false,
  budgets, budgetRules: Object.fromEntries(Object.entries(budgets).map(([name, value]) => [name, rule(value)])), requestedBudgets: budgets,
  turnBudgets: true, imports: [], resources: [], fairFuel: true,
};

const directory = await mkdtemp(join(tmpdir(), 'varta-loader-'));
after(() => rm(directory, {recursive: true, force: true}));
const source = name => readFile(new URL(name, import.meta.url), 'utf8');
const loaderPath = join(directory, 'varta-host.js');
await writeFile(loaderPath, [
  ...Object.entries(MODULES).map(([name, file]) => `import * as ${name} from ${JSON.stringify(new URL(file, import.meta.url).href)};`),
  `const descriptor = ${JSON.stringify(descriptor)};`,
  'const registry = {network:descriptor.network,view:descriptor.view,permissions:descriptor.permissions,environment:descriptor.environment};',
  await source('network-policy.js'), await source('loader.js'),
].join('\n'));
const loader = await import(pathToFileURL(loaderPath).href);

// The page's Worker and fetch: verified bundle files and one fake Worker.
let worker, started;
class FakeWorker {
  constructor() { worker = this; this.messages = []; this.terminated = false; }
  postMessage(message) { this.messages.push(message); if (this.messages.length === 1) started(); }
  terminate() { this.terminated = true; }
  send(data) { this.onmessage({data}); }
}
const delays = [];
const realSetTimeout = globalThis.setTimeout;
const previous = {Worker: globalThis.Worker, fetch: globalThis.fetch, setTimeout: globalThis.setTimeout};
globalThis.Worker = FakeWorker;
globalThis.fetch = async url => new Response(files[new URL(url).pathname.split('/').at(-1)]);
globalThis.setTimeout = (callback, delay, ...rest) => { delays.push(delay); return realSetTimeout(callback, delay, ...rest); };
after(() => Object.assign(globalThis, previous));

const wait = ms => new Promise(done => realSetTimeout(done, ms));
async function mount(options = {}) {
  const ready = new Promise(done => { started = done; });
  const app = loader.mount({budgetCeiling: {[C.deadlineBudget]: IDLE_DEADLINE_MS}, ...options});
  await ready;
  return app;
}
// Main's checkpoint: the application becomes ready.
const checkpoint = async (app) => { worker.send({type: 'turn_end', id: 1, usage: {}}); await app.ready; };
const exhausted = turn => C.deadlineExhausted.replace(C.turnPlaceholder, String(turn)).replace(C.limitPlaceholder, String(IDLE_DEADLINE_MS));

test('maintenance_begin arms the idle deadline with the native diagnostic and terminates the Worker', {timeout: 4000}, async () => {
  const app = await mount();
  await checkpoint(app);
  const armed = delays.length;
  worker.send({type: 'maintenance_begin', turn: 1});
  assert.equal(delays[armed], IDLE_DEADLINE_MS);
  await assert.rejects(app.finished, {message: exhausted(1)});
  assert.equal(worker.terminated, true);
});

test('maintenance_end clears the idle deadline and the session continues', {timeout: 4000}, async () => {
  const app = await mount();
  await checkpoint(app);
  worker.send({type: 'maintenance_begin', turn: 1});
  worker.send({type: 'maintenance_end', turn: 1});
  let settled = false;
  app.finished.then(() => { settled = true; }, () => { settled = true; });
  await wait(IDLE_DEADLINE_MS * CLEARED_WAIT_PERIODS);
  assert.equal(settled, false);
  assert.equal(worker.terminated, false);
  app.close();
  assert.equal((await app.finished).status, 'closed');
  assert.equal(worker.terminated, true);
});

test('a maintenance message with an invalid turn or in a bounded run is a protocol failure that arms nothing', {timeout: 4000}, async () => {
  const protocolFailure = {message: 'invalid browser worker message'};
  for (const turn of [0, -1, 1.5, '1', undefined]) {
    const app = await mount();
    await checkpoint(app);
    const armed = delays.length;
    worker.send({type: 'maintenance_begin', turn});
    assert.equal(delays.length, armed, String(turn));
    await assert.rejects(app.finished, protocolFailure);
    assert.equal(worker.terminated, true);
  }
  // A bounded run collects nothing between turns.
  const ready = new Promise(done => { started = done; });
  const run = loader.run({budgetCeiling: {[C.deadlineBudget]: IDLE_DEADLINE_MS}});
  await ready;
  const armed = delays.length;
  worker.send({type: 'maintenance_begin', turn: 1});
  assert.equal(delays.length, armed);
  await assert.rejects(run, protocolFailure);
  assert.equal(worker.terminated, true);
});

test('an exit before the first checkpoint rejects readiness with the exit code', {timeout: 4000}, async () => {
  const app = await mount();
  worker.send({type: 'result', status: 'ok', logs: [], exitCode: EXIT_CODE});
  await assert.rejects(app.ready, {message: `browser application exited with code ${EXIT_CODE}`});
  const result = await app.finished;
  assert.equal(result.status, 'ok');
  assert.equal(result.exitCode, EXIT_CODE);
});

test('an exit after the first checkpoint leaves the resolved readiness unchanged', {timeout: 4000}, async () => {
  const app = await mount();
  await checkpoint(app);
  worker.send({type: 'result', status: 'ok', logs: [], exitCode: EXIT_CODE});
  assert.equal((await app.finished).exitCode, EXIT_CODE);
  await app.ready;
});
