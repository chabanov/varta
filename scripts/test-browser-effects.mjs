// Native/browser equivalence of the pilot effects (Date.now, new Date,
// Math.random, crypto.getRandomValues, crypto.randomUUID, process.env,
// process.exit) and of idle collection between the turns of a long session,
// including their recoverable failures (an invalid exit code, a too-large
// environment snapshot) and the older profiles that import a pilot effect
// natively (v37's clock, entropy and exit, v20's entropy and exit).
// Each program runs natively (`varta dev --budget-report`, `--app` for the
// session) and in the browser host served by `varta dev --target browser`;
// logs, exit codes and exact fuel, host-call, microtask, output and
// maintenance usage must be equal. Random values are checked by the guest
// for range and shape only, so its work does not depend on them. The one
// exception is crypto.randomUUID: the guest prelude's own cost depends on the
// drawn bytes (native runs of one program differ by whole remainder paths), so
// its case compares everything except fuel.
//
//   node scripts/test-browser-effects.mjs <varta-executable>
//
// Real local Chrome (VARTA_CHROME_BIN overrides the path). Other engines run
// the same cases through scripts/test-browser-matrix.mjs --effects.
import {mkdtemp, mkdir, writeFile, rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import {spawn, spawnSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import assert from 'node:assert/strict';
import {registry} from '../crates/varta-common/browser/registry.mjs';

const PROFILE = 'checked-f64-utf16-v38';
const CLOCK_PROFILE = 'checked-f64-utf16-v37';
const ENTROPY_PROFILE = 'checked-f64-utf16-v20';
const HOST_ABI = registry.profiles.find((profile) => profile.name === PROFILE).hostAbi;
const COLLECTOR = registry.effects.collector;
const EXIT_CODE = 3;
// Not an integer: a recoverable invalid-input failure on both hosts.
const INVALID_EXIT_CODE = 1.5;
const SESSION_TIMERS = 12;
// Long enough that every timer turn ends before the next timer is due, so
// each gap is one idle period on both hosts.
const SESSION_DELAY_MS = 60;
const SESSION_ROWS = 400;
const STARTUP_POLLS = 500, STARTUP_POLL_MS = 30;

const EFFECTS = `export function main(): void {
  try {
    const before = Date.now();
    console.log(Number.isInteger(before) && before > 1600000000000);
    const date = new Date();
    console.log(date.getTime() >= before);
  } catch (error) {
    console.log(error instanceof Error ? error.name : 'other');
  }
  let inRange = true;
  for (let i = 0; i < 64; i++) {
    const sample = Math.random();
    if (!(sample >= 0 && sample < 1)) { inRange = false; }
  }
  console.log(inRange);
  const bytes = crypto.getRandomValues(new Uint8Array(32));
  let bytesOk = bytes.length === 32;
  for (const byte of bytes) {
    if (!(Number.isInteger(byte) && byte >= 0 && byte <= 255)) { bytesOk = false; }
  }
  console.log(bytesOk);
  console.log(JSON.stringify(globalThis.process.env));
  console.log(globalThis.process.env.VARTA_EFFECT_WILD_X ?? 'unset');
  console.log(globalThis.process.env.VARTA_EFFECT_SECRET ?? 'unset');
  varta.process.exit(${EXIT_CODE});
  console.log('after exit');
}
`;

const UUID = `export function main(): void {
  for (let i = 0; i < 4; i++) {
    const id = crypto.randomUUID();
    console.log(id.length === 36 && id[8] === '-' && id[13] === '-' && id[14] === '4' && id[18] === '-' && id[23] === '-');
  }
}
`;

// Every timer turn builds rows of strings and arrays and keeps only the last
// batch, so each turn allocates well past collector::IDLE_MIN_BYTES.
const SESSION = `class Row { constructor(public label: string, public cells: string[]) {} }
let kept: Row[] = [];
let count = 0;
function tick(): void {
  const rows: Row[] = [];
  for (let i = 0; i < ${SESSION_ROWS}; i++) {
    const cells: string[] = [];
    for (let j = 0; j < 4; j++) { cells.push(\`\${count}-\${i}-\${j}\`); }
    rows.push(new Row(\`row \${i}\`, cells));
  }
  kept = rows;
  count++;
  console.log(\`tick \${count} \${kept.length}\`);
  if (count < ${SESSION_TIMERS}) { setTimeout(tick, ${SESSION_DELAY_MS}); }
}
export function main(): void { setTimeout(tick, ${SESSION_DELAY_MS}); }
`;

// The exits of the older-profile cases: an invalid code, then an exit.
const EXITS = `  varta.process.exit(${INVALID_EXIT_CODE});
  console.log('after invalid exit');
  varta.process.exit(${EXIT_CODE});
  console.log('after exit');
`;
const ENTROPY = `  let inRange = true;
  for (let i = 0; i < 64; i++) {
    const sample = Math.random();
    if (!(sample >= 0 && sample < 1)) { inRange = false; }
  }
  console.log(inRange);
`;
// Granted, so the clock cannot throw (a v37 catch would import the
// error-class operation, which the browser host does not implement).
const CLOCK_V37 = `export function main(): void {
  const now = Date.now();
  console.log(Number.isInteger(now) && now > 1600000000000);
${ENTROPY}${EXITS}}
`;
const ENTROPY_V20 = `export function main(): void {
${ENTROPY}${EXITS}}
`;

// Granted ASCII values whose snapshot's UTF-16 result exceeds the native
// result limit (registry.environment.maxResultBytes); each one alone fits,
// and each stays below the per-string limit of a Linux process environment.
const UTF16_UNIT_BYTES = Uint16Array.BYTES_PER_ELEMENT;
const LARGE_VALUE_BYTES = 96 * 1024;
const LARGE = Array.from({length: Math.floor(registry.environment.maxResultBytes / UTF16_UNIT_BYTES / LARGE_VALUE_BYTES) + 1},
  (_, index) => [`VARTA_EFFECT_LARGE_${index}`, 'x'.repeat(LARGE_VALUE_BYTES)]);
const TOO_LARGE = `export function main(): void {
  console.log(Object.keys(globalThis.process.env).length);
  console.log((globalThis.process.env.${LARGE[0][0]} ?? '').length);
  varta.process.exit(${INVALID_EXIT_CODE});
  console.log('after invalid exit');
}
`;

const ENVIRONMENT = [['VARTA_EFFECT_ONE', 'Привіт 😀'], ['VARTA_EFFECT_TWO', 'two "quoted"\n'], ['VARTA_EFFECT_WILD_X', 'wild'], ['VARTA_EFFECT_SECRET', 'PRIVATE']];
const GRANTED = {time: {now: true}, env: {get: ['VARTA_EFFECT_ONE', 'VARTA_EFFECT_TWO', 'VARTA_EFFECT_ABSENT', 'VARTA_EFFECT_WILD_*']}, process: {exit: true}};

/** The cases: a project, its native arguments and the browser options. */
export const CASES = [
  {name: 'effects', source: EFFECTS, capabilities: GRANTED, exit: EXIT_CODE, clockReads: 2},
  // An empty host ceiling denies the clock, the environment and the exit
  // before any effect; entropy stays available (it is ungated).
  {name: 'effects-empty-ceiling', source: EFFECTS, capabilities: GRANTED, policy: [], ceiling: {}, exit: 0, denied: true},
  // Without the package grants nothing is requested, so nothing is allowed.
  {name: 'effects-no-grant', source: EFFECTS, capabilities: {}, exit: 0, denied: true},
  {name: 'uuid', source: UUID, capabilities: {}, exit: 0, valueDependentFuel: true},
  {name: 'session', source: SESSION, capabilities: {time: {timer: true}}, session: true, exit: 0},
  // A host maintenance ceiling below one idle step's fuel: both hosts run no
  // step, list no period and keep the same turns.
  {name: 'session-maintenance-ceiling', source: SESSION, capabilities: {time: {timer: true}}, session: true, exit: 0,
    policy: [{id: 'time.timer', enabled: true}], ceiling: {'time.timer': true}, budgets: {[COLLECTOR.fuelBudget]: COLLECTOR.stepFuel - 1}},
  // The snapshot is too large and the exit code invalid: both recoverable,
  // with the native diagnostics; a static name still reads its variable.
  {name: 'failures', source: TOO_LARGE, environment: LARGE, capabilities: {env: {get: LARGE.map(([name]) => name)}, process: {exit: true}}, exit: 0,
    diagnostics: [registry.effects.failures.tooLarge, registry.effects.failures.invalid]},
  // Older profiles keep exactly the pilot effects their native host imports.
  {name: 'v37-clock-entropy-exit', profile: CLOCK_PROFILE, source: CLOCK_V37, capabilities: {time: {now: true}, process: {exit: true}}, exit: EXIT_CODE, clockReads: 1,
    diagnostics: [registry.effects.failures.invalid]},
  {name: 'v20-entropy-exit', profile: ENTROPY_PROFILE, source: ENTROPY_V20, capabilities: {process: {exit: true}}, exit: EXIT_CODE,
    diagnostics: [registry.effects.failures.invalid]},
];

async function project(root, item) {
  const directory = join(root, item.name);
  await mkdir(directory);
  await writeFile(join(directory, 'main.ts'), item.source);
  await writeFile(join(directory, 'varta.config.json'), JSON.stringify({entry: 'main.ts', target: 'wasm', out: 'dist/app', languageProfile: item.profile ?? PROFILE, capabilities: item.capabilities}));
  if (item.policy) await writeFile(join(directory, 'host-policy.json'), JSON.stringify({schemaVersion: 1, abiVersion: item.budgets ? HOST_ABI : 1, capabilities: item.policy, ...(item.budgets ? {budgets: item.budgets} : {})}));
  return directory;
}

const number = Number;
function nativeRun(varta, directory, item) {
  const environment = {...process.env};
  for (const name of Object.keys(environment)) if (name.startsWith('VARTA_EFFECT_')) delete environment[name];
  for (const [name, value] of item.environment ?? ENVIRONMENT) environment[name] = value;
  const args = [...(item.policy ? ['--policy', 'host-policy.json'] : []), 'dev', '--budget-report', ...(item.session ? ['--app'] : [])];
  const run = spawnSync(varta, args, {cwd: directory, encoding: 'utf8', env: environment});
  if (run.error) throw run.error;
  // Turn budgets (v38) report each turn; earlier profiles one execution.
  const turns = [...run.stderr.matchAll(/varta budget: (?:turn (\d+) (\w+)|(execution)): fuel (\d+)\/\d+, host calls (\d+)\/\d+, microtasks (\d+),/g)]
    .map(([, turn, kind, execution, fuel, hostCalls, microtasks]) => ({turn: number(turn ?? 1), kind: kind ?? execution, fuel: number(fuel), hostCalls: number(hostCalls), microtasks: number(microtasks)}));
  const maintenance = [...run.stderr.matchAll(/varta budget: maintenance after turn (\d+): fuel (\d+)\/\d+, steps (\d+), cycle (completed|continues), wall \d+ ms \(\d+ ms per idle period\), memory (\d+)\/\d+ bytes/g)]
    .map(([, afterTurn, fuel, steps, cycle, memoryBytes]) => ({afterTurn: number(afterTurn), fuel: number(fuel), steps: number(steps), completed: cycle === 'completed', memoryBytes: number(memoryBytes)}));
  const total = /varta budget: total: (\d+) turns, fuel (\d+), peak memory \d+\/\d+ bytes, output (\d+)\/\d+ bytes/.exec(run.stderr);
  const maintenanceTotal = /varta budget: maintenance total: (\d+) idle periods, fuel (\d+)/.exec(run.stderr);
  assert(total, run.stderr);
  if (/later turns not listed/.test(run.stderr)) throw new Error('native report truncated');
  return {
    exitCode: run.status, logs: run.stdout.split(/\r?\n/).filter((line) => line.startsWith('varta log: ')), stderr: run.stderr,
    turns, maintenance, totalTurns: number(total[1]), fuel: number(total[2]), outputBytes: number(total[3]),
    maintenancePeriods: number(maintenanceTotal?.[1] ?? 0), maintenanceFuel: number(maintenanceTotal?.[2] ?? 0),
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
  const options = JSON.stringify({environmentInputs: item.environment ?? ENVIRONMENT, ...(item.ceiling ? {permissionCeiling: item.ceiling} : {}), ...(item.budgets ? {budgetCeiling: item.budgets} : {})});
  const pick = `{status:r.status,error:r.error,exitCode:r.exitCode,logs:r.logs,fuelUsed:r.fuelUsed,hostCalls:r.hostCalls,microtasks:r.microtasks,outputBytes:r.outputBytes,turns:r.turns,maintenance:r.maintenance,maintenancePeriods:r.maintenancePeriods,maintenanceFuel:r.maintenanceFuel,audit:r.permissions.audit.map(a=>[a.operation,a.outcome])}`;
  return item.session
    ? `(async()=>{const {mount}=await import(document.querySelector('script[data-varta-dev]').src);const usages=[];const app=mount({...${options},onTurn:u=>usages.push(u)});const r=await app.finished;return {...${pick},usages};})()`
    : `(async()=>{const {run}=await import(document.querySelector('script[data-varta-dev]').src);const r=await run(${options});return ${pick};})()`;
}

function compare(item, native, browser) {
  const label = (what) => `${item.name}: ${what}`;
  assert.equal(browser.status, 'ok', label(browser.error));
  assert.equal(browser.exitCode, native.exitCode, label('exit code'));
  assert.equal(browser.exitCode, item.exit, label('expected exit code'));
  assert.deepEqual(browser.logs, native.logs, label('logs'));
  assert(!JSON.stringify(browser).includes('PRIVATE'), label('ungranted variable reached the page'));
  assert.equal(browser.turns, native.totalTurns, label('turns'));
  if (!item.valueDependentFuel) assert.equal(browser.fuelUsed, native.fuel, label('fuel'));
  assert.equal(browser.outputBytes, native.outputBytes, label('output bytes'));
  assert.equal(browser.hostCalls, native.turns.reduce((sum, turn) => sum + turn.hostCalls, 0), label('host calls'));
  assert.equal(browser.microtasks, native.turns.reduce((sum, turn) => sum + turn.microtasks, 0), label('microtasks'));
  assert.equal(browser.maintenancePeriods, native.maintenancePeriods, label('maintenance periods'));
  assert.equal(browser.maintenanceFuel, native.maintenanceFuel, label('maintenance fuel'));
  assert.deepEqual(browser.maintenance, native.maintenance, label('maintenance'));
  if (item.session) {
    // Per-turn usage from the cumulative checkpoint reports.
    const turns = browser.usages.map((usage, index) => {
      const previous = browser.usages[index - 1] ?? {fuelUsed: 0, hostCalls: 0, microtasks: 0};
      return {fuel: usage.fuelUsed - previous.fuelUsed, hostCalls: usage.hostCalls - previous.hostCalls, microtasks: usage.microtasks - previous.microtasks};
    });
    assert.deepEqual(turns, native.turns.map(({fuel, hostCalls, microtasks}) => ({fuel, hostCalls, microtasks})), label('per-turn usage'));
    assert.equal(native.maintenance.length > 0, !item.budgets, label('the session collects between turns unless the ceiling forbids a step'));
  }
  // The clock is an audited broker operation (after any initial state record);
  // the snapshot and the exit, like native, are not.
  const clock = browser.audit.filter(([operation]) => operation === 'time.now').map(([, outcome]) => outcome);
  const unaudited = browser.audit.filter(([operation]) => operation === 'process.exit' || operation === 'env.get').length;
  if (item.denied) {
    assert(native.logs.includes('varta log: NotAllowedError'), label('clock denial'));
    assert(native.logs.includes('varta log: {}'), label('empty snapshot'));
    assert(native.logs.includes('varta log: after exit'), label('denied exit continues'));
    assert(clock.length >= 1 && clock.every((outcome) => outcome === 'denied'), label('clock audit'));
  } else if (item.exit !== 0) {
    assert(!native.logs.includes('varta log: after exit'), label('exit ends the run'));
  }
  if (!item.denied && item.clockReads) assert(clock.length >= item.clockReads && clock.every((outcome) => outcome === 'granted'), label('clock audit'));
  // The registry's failure texts are the native diagnostics (the browser's
  // logs equal native's above), and execution continues after each.
  for (const text of item.diagnostics ?? []) assert(native.logs.includes(`varta log: ${text}`), label(`native diagnostic ${text}`));
  if (item.diagnostics?.includes(registry.effects.failures.invalid)) assert(native.logs.includes('varta log: after invalid exit'), label('an invalid exit continues'));
  assert(unaudited <= (item.capabilities.env ? 1 : 0) + (item.capabilities.process ? 1 : 0), label('unaudited effects'));
}

/** Run every case; `open(url, evaluate => …)` drives one browser page. */
export async function runEffects(varta, open) {
  const root = await mkdtemp(join(tmpdir(), 'varta-browser-effects-'));
  const passed = [];
  try {
    for (const item of CASES) {
      const directory = await project(root, item);
      const native = nativeRun(varta, directory, item);
      const {url, child} = await serve(varta, directory);
      try {
        const browser = await open(url, (evaluate) => evaluate(expression(item)));
        compare(item, native, browser);
      } finally { await stop(child); }
      passed.push(item.name);
    }
  } finally { await rm(root, {recursive: true, force: true}); }
  return passed;
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  if (process.argv.length !== 3) throw new Error('Usage: node scripts/test-browser-effects.mjs <varta-executable>');
  const {withBrowserPage} = await import('./browser-probe.mjs');
  const passed = await runEffects(resolve(process.argv[2]), (url, action) => withBrowserPage(url, (call) => action(async (text) => {
    const response = await call('Runtime.evaluate', {expression: text, awaitPromise: true, returnByValue: true});
    if (response.exceptionDetails) throw new Error(JSON.stringify(response.exceptionDetails));
    return response.result.value;
  })));
  console.log(`Chrome effect acceptance passed (${passed.join(', ')}): logs (including the registry's failure diagnostics), exit codes, exact host-call/microtask/output usage, exact fuel (except the value-dependent randomUUID prelude) and idle maintenance equal native`);
}
