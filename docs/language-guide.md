# Varta language guide

A user-facing tour of the checked TypeScript subset, with examples. The normative
contract for every feature is in [architecture.md](architecture.md); the current
profile list and versions are in its [version matrix](architecture.md#version-matrix).
Select a profile with `languageProfile` in `varta.config.json`. Each opt-in profile
inherits everything from the profile before it.

## Projects, permissions and execution

### Project and permissions

```text
varta.config.json
src/main.ts
varta.d.ts
dist/app.wasm
dist/app.manifest.json
dist/app                 # build --single
```

Example `varta.config.json`:

```json
{
  "entry": "src/main.ts",
  "target": "native",
  "policy": "strict",
  "out": "dist/app",
  "capabilities": {
    "time": { "now": true },
    "env": { "get": ["APP_NAME"] },
    "fs": { "read": ["/data/**"] },
    "net": { "fetch": ["http://example.com/public/**"] }
  }
}
```

Omitted grants, empty lists and false deny access. Permissions for read/print,
get/has and fetch/get are independent. Paths retain directory boundaries and are
opened through pinned descriptors with symlinks rejected. Network authority is
exact; URLs are validated before I/O, with redirects and ambient proxies disabled.
Only the strict profile and HTTP transport are implemented.

`inspect --json` shows full permission scopes. Without a host policy the CLI uses
the package's declared grants. To restrict an untrusted package, supply a separate
host policy in the manifest format:

```json
{
  "capabilities": [
    { "id": "env.get", "patterns": ["APP_NAME"] }
  ]
}
```

```sh
varta run dist/app.wasm --policy host-policy.json
varta inspect dist/app.wasm --policy host-policy.json --json
# A newly built native launcher accepts the same option:
./dist/app --policy host-policy.json
```

An operation must satisfy both the package request and the host policy. Empty
host capabilities deny everything; malformed or missing policy files fail before
execution. The host cannot add unrequested rights, and the package cannot override
the host ceiling. Inspection shows both filters and distinguishes unconditional
denials from resource checks performed at runtime. `--policy` also works with
`dev` and `validate`; it is never saved into a build. Native executables themselves
run with the launching user's OS authority.

### Language and execution

The subset includes local named-function modules, typed primitive/object functions,
lexical let/const bindings, integer arithmetic, boolean operations, conditions,
loops and switch. Numbers use signed i32 semantics by default or binary64 with the
explicit profile above. Strings work as variables,
parameters, explicit function results and dynamic host inputs:

```ts
function read(name: string): string { return varta.env.get(name); }
export function main(): void {
  const name = read("APP_NAME");
  const other = read("OTHER_NAME");
  console.log(name); // Still holds the first result.
  console.log(other);
}
```

Host results are copied into immutable, bounded storage. Unreachable strings are
collected and their storage is reused during execution; aliases and saved results
remain valid. Every read still needs the corresponding capability. Strings support `+`,
`==`, `!=`, `===`, `!==`, `<`, `<=`, `>` and `>=`:

```ts
export function main(): void {
  const message = "Привіт" + " 🦀";
  console.log(message == "Привіт 🦀"); // 1
  console.log("apple" < "banana");   // 1
}
```

Comparisons use content and UTF-16 code-unit order, without locale sorting or
normalization. Both operands must be strings; numbers are not converted to text.
One string is limited to 1 MiB of stored bytes (UTF-8 or UTF-16LE for the selected
profile). Allocations share the memory/fuel
budgets. Unused expression values, overwritten bindings and out-of-scope locals
can be reclaimed. Live values stay in place; committed memory pages remain until
the run ends. See the [reclamation contract](architecture.md#string-memory-reclamation).
See the [operator contract](architecture.md#string-operators) and
[ownership contract](architecture.md#string-values-and-ownership).
Unsupported constructs, top-level effects and type mismatches fail
explicitly. There is no general Node/npm compatibility or async execution yet.

Runtime defaults include 8 MiB guest memory, instruction fuel, a 5-second
execution/HTTP deadline, and bounded host calls, I/O results and output. Host
failures clear result lengths and expose status through `varta.result.status()`:
0 success, 1 denied, 2 invalid input, 3 I/O failure, 4 result too large.

### Local modules

Split functions across files using explicit relative `.ts` paths:

```ts
// src/math.ts
export function twice(n: number): number { return n * 2; }

// src/main.ts
import { twice as double } from "./math.ts";
export function main(): void { console.log(double(21)); }
```

Named function imports/exports and local export aliases are supported. Private
functions keep their own file scope. The entry file declares `main(): void`;
dependencies are linked into the Wasm and native artifacts at build time.
`varta check --json` includes the resolved dependency graph in `modules`.

Dependencies must stay within the configuration directory; symlinks and cycles
are rejected. Default/namespace imports, export-from, dynamic imports, npm and
URL imports are not supported. See the [module contract](architecture.md#local-modules)
for path rules and graph limits.

### Diagnostics

`check`, `build` and `dev` report the actual source file and the failing source
range, including errors inside nested expressions:

```text
/path/to/app/src/main.ts:2:19: type mismatch for x: expected string, found number
  |   let x: string = 7;
  |                   ^
```

`varta check --json` returns structured `diagnostics` with category, filename,
line/column range, UTF-8 byte offsets and a bounded excerpt. Unicode, CRLF and BOM
positions are supported. See the [diagnostic contract](architecture.md#source-diagnostics)
for coordinate conventions and public compiler APIs.

### Programmatic host errors

Capture a direct system call to keep its value and status together:

```ts
export function main(): void {
  const result = varta.result.capture(varta.env.get("APP_NAME"));
  if (result.ok) {
    console.log(result.value);
  } else {
    console.log(result.status);
  }
}
```

The readonly `ok`, `status` and `value` fields survive later system calls. Results
can be passed or returned as `VartaResult<string>`, `VartaResult<number>`,
`VartaResult<boolean>` or `VartaResult<void>`. Capture the system call inside a
user wrapper before returning it. Existing permission checks still apply, and
resource exhaustion still ends execution. The old API and status inspection stay
available. See the [result contract](architecture.md#programmatic-host-results).

## Default profile values

### Objects and JSON

For distinct `undefined`, select `"languageProfile":"checked-f64-utf8-v2"`.
It keeps binary64/UTF-8 and adds the `undefined` type, global value, `void`, typed
`typeof`, nullish comparisons and values from void calls. For example:

```ts
export function main(): void {
  const u = undefined;
  console.log(u === null);                 // 0
  console.log(u == null);                  // 1
  console.log(JSON.stringify({u, n:1.5})); // {"n":1.5}
  console.log(JSON.stringify([u, u]));     // [null,null]
  console.log(JSON.stringify(u));          // undefined
}
```

Root `JSON.stringify(undefined)` returns an actual undefined value. Object fields
are omitted, array elements become null, and empty strings/null/zero keep their
distinct meanings. This profile requires host ABI v3/value ABI v4; earlier
profiles retain their behavior. Dynamic unions, missing-property values,
uninitialized locals and generic coercion remain pending; checked JSON cannot
parse an undefined target because JSON has no such value.

The following structured-value features work in either numeric profile, using
versioned field tags and JSON schemas. Default i32 behavior remains unchanged.

```ts
export function main(): void {
  const value = { count: 1, label: "ready", meta: { ok: true, empty: null } };
  const alias = value;
  alias.count = 2;
  console.log(JSON.stringify(value));
  // {"count":2,"label":"ready","meta":{"ok":true,"empty":null}}
}
```

Objects preserve identity, aliases, nested references and mutable typed fields.
Inline object annotations work across functions/modules; field order does not
change the type. Unreachable objects and their contained strings are collected.
`JSON.stringify(value)` emits compact JSON with bounded depth, traversal and
encoded bytes. Serialization is pure and needs no system permission.

These profiles have closed shapes with at most 32 fields, their selected numeric representation,
valid UTF-8 strings, booleans, nested objects/arrays and `null`. Optional fields,
dynamic keys/shapes, prototypes, methods and named object type aliases
are not supported yet. Undefined fields require the v2 binary64 profile above.
Checked JSON parsing is available below; dynamic parsing,
replacers and indentation remain pending.
In the default i32 profile, Tool Contract v1 accepts primitive signatures; a tool may build an
object internally and return its serialized string. See the
[structured value contract](architecture.md#structured-values-and-json).

### Typed arrays

```ts
function total(values: number[]): number {
  let result = 0;
  for (let i = 0; i < values.length; i = i + 1) {
    result = result + values[i];
  }
  return result;
}
export function main(): void {
  const values = [1, 2, 3];
  const alias = values;
  alias[1] = 7;
  const empty: string[] = [];
  console.log(total(values)); // 11
  console.log(JSON.stringify({ values, empty })); // {"values":[1,7,3],"empty":[]}
}
```

Dense arrays have one exact element type and preserve identity across aliases,
functions and modules. Both `T[]` and `Array<T>` annotations work, including nested
arrays/objects and typed empty arrays. The initial profile has a fixed length of
at most 1024 elements: indexed replacement is supported; push/pop, resizing,
sparse arrays, tuples and mixed element types remain unsupported. An invalid
index traps instead of producing JavaScript `undefined` or creating a new property.
Indices must be exact nonnegative integers; binary64 indices are checked before
integer conversion. String-key coercion is not implemented. Values are
traced by the collector and serialize as ordinary JSON arrays. See the
[array contract](architecture.md#dense-arrays).

### Checked JSON parsing

```ts
export function main(): void {
  const value = varta.json.parse<{ count: number; labels: string[] }>(
    '{"labels":["ready"],"count":1}'
  );
  value.count = value.count + 1;
  console.log(JSON.stringify(value)); // {"count":2,"labels":["ready"]}
}
```

The explicit supported type generates a real runtime schema. Parsing rejects
malformed JSON, duplicate/unknown/missing fields, wrong types and limit violations
before allocating guest values. It produces ordinary owned values, with the same
mutation, aliases, collection and cross-module behavior as literals. It needs no
system permission and preserves the last system status.

The initial checked profile accepts at most 64 KiB of encoded input, depth 64,
4096 values and 1024 elements per array. The default profile requires plain signed
i32 integer notation and rejects fractional/exponent/negative-zero forms. The
binary64 profile accepts finite JSON numbers and preserves negative zero. Object fields are
normalized to schema order (integer-index keys first, other keys in canonical name
order). This checked extension has no reviver, dynamic target, optional fields or
general `JSON.parse` API yet. Invalid input traps; named invocation reports
`execution_failed` without a partial result. See the
[parsing contract](architecture.md#checked-json-parsing).

## Services and tools

### HTTP server

```ts
function handle(request: VartaHttpRequest): VartaHttpResponse {
  return {
    status: 200,
    headers: [{ name: "content-type", value: "application/json" }],
    body: JSON.stringify({ ok: true, target: request.target })
  };
}
export function main(): void {}

varta.http.serve({ hostname: "127.0.0.1", port: 8080 }, handle);
```

Declare the exact listener in `varta.config.json`:

```json
"capabilities": { "net": { "listen": ["127.0.0.1:8080"] } }
```

`varta dev`, `varta run` and `build --single` artifacts start this service after
`main` completes. The CLI reports the listening address on stderr. Port 0 chooses
an OS-assigned port and requires an explicit `127.0.0.1:0` grant. Host policies must
also permit the same requested endpoint. Outbound grants never authorize binding.

The top-level declaration accepts literal IP/port options and one named handler,
including an imported function. HTTP publication is independent of AI tools/MCP.
Request records contain `method`, `target` (path/query), UTF-8 `body` and a header
array. Response records contain `status`, UTF-8 `body` and headers. The body above
is sent as ordinary JSON, without a tool envelope. Authentication, routes and
payload rules belong in the TS handler.

The first profile processes one connection at a time and closes it after one
response. Each request gets a fresh Wasm instance, 8 MiB memory, fuel and a
five-second execution budget. `main` is not rerun. A failed handler yields a generic
500 and subsequent requests can proceed. Bodies are limited to 8 KiB, headers to
32 / 4 KiB, targets to 1 KiB and encoded responses to 1 MiB. Streaming, keep-alive,
shared guest state, general async JS, TLS and HTTP/2 remain pending.

`varta.http.stop()` in the owning handler stops acceptance after the current reply
finishes. Embedders can use `run_observed` and the shutdown handle in
`RuntimeEvent::Listening`. The CLI streams bounded request logs; the collecting
runtime API retains at most 1 MiB of lifetime output. See the
[HTTP contract](architecture.md#http-service-v1).

### Public tool contracts

Publish selected exported functions with a compile-time entry-module declaration:

```ts
export function echo(value: string): string {
  return value;
}
export function main(): void {}

varta.tool({
  name: "text.echo",
  description: "Return the supplied text"
}, echo);
```

Build normally, then read the catalog without running the application:

```sh
varta build --single
varta inspect dist/app.wasm --json
varta inspect dist/app --json
```

`toolCatalog` includes generated input/output schemas, required system operations
and limits. Metadata lives inside Wasm and survives native packaging and removal
of source files. Ordinary exports do not become tools. Imported functions may be
published through an explicit entry-module export. Parameters support `number`
(signed i32), `boolean` and `string`; results also support `void`. Output schemas
wrap the result as `{ "value": ... }`.

Catalog versions, sizes, handler signatures and transitive system requirements are
validated on check/build/dev/run/invoke/inspect/validate. Publishing a tool grants no
permissions. Generic application state and MCP remain planned; this example declares
a stateless tool. See the
[tool contract](architecture.md#tool-contract-v1-and-offline-discovery).

### Invoke a tool

```sh
varta invoke dist/app.wasm text.echo --params '{"value":"Hello"}' --json
varta invoke dist/app text.echo --params '{"value":"Hello"}' --json
```

Both commands return one JSON record:

```json
{"status":"ok","result":{"value":"Hello"},"logs":[]}
```

Invocation runs only the published handler, with a fresh instance and the existing
memory, fuel, time and host-call limits. It does not call `main`. All parameters
are checked before guest execution: one object, exact required fields, no duplicate
or unknown fields, no coercion, signed i32 integers, booleans and UTF-8 strings.
The input is limited to 64 KiB, including JSON whitespace/escaping. Integer input
uses plain integer notation; decimal/exponent and negative-zero forms are rejected.
The encoded `{ "value": ... }` result is limited to 1 MiB; logs have their own
existing output limit. Strings are copied into host-owned results.

`--policy host-policy.json` also restricts invocation. Concrete system operations
still require both package and host grants. A handler can recover from denied
operations through `VartaResult`; that does not grant access. Boundary failures
return exit code 1 and, with `--json`, a stable `code` such as `invalid_arguments`,
`unknown_tool`, `invalid_result` or `execution_failed`. File/configuration errors
before dispatch keep the ordinary CLI error handling. Without `--json`, stdout
contains only `{ "value": ... }` and application logs go to stderr.

This local interface does not provide remote authentication, persistent application
state or rollback of host effects. Failed calls are not retried automatically.

## Opt-in profiles

### `checked-f64-utf8-v1`

#### Binary64 numbers (opt-in)

Add `"languageProfile":"checked-f64-utf8-v1"` to your project configuration:

```json
{
  "entry": "src/main.ts",
  "target": "native",
  "out": "dist/app",
  "languageProfile": "checked-f64-utf8-v1",
  "capabilities": {}
}
```

```ts
export function main(): void {
  console.log(7 / 2);             // 3.5
  console.log(2147483647 + 1);    // 2147483648
  console.log(1 / -0);            // -Infinity
  console.log(-5.5 % 2);          // -1.5
}
```

This profile supports fractional/large numbers, NaN/Infinity, signed zero, numeric
comparisons/truthiness, functions, modules and control flow. It uses host ABI v2;
numbers occupy f64 slots and strings retain their existing UTF-8 representation.
Use the normal check/dev/build/run commands, including source-free native builds.
`process.exit` accepts only an exact finite i32 code and still requires permission.

Typed objects, dense fixed-length arrays, `JSON.stringify` and checked
`varta.json.parse<T>` now work with binary64 through structured-value ABI v3.
Numeric fields/elements retain signed zero and full precision. JSON emits
ECMAScript decimal notation, maps NaN/infinities to null and serializes -0 as 0.
Checked parsing accepts finite fractional/exponent numbers, with duplicate/unknown/
missing-field rejection. Overflowing numbers are rejected by this strict API.
Array indices must be exact nonnegative integers within bounds; invalid indices trap.

Tool/HTTP adapters, UTF-16 and general mixed-type coercion remain pending for
the newer profiles. Distinct undefined requires the v2 profile below. Omit `languageProfile` to keep the existing i32 profile and
its structured-value/server/tool features. Diagnostic number logs preserve -0 and
name NaN/Infinity; their finite formatting is not a Number::toString API.

### `checked-f64-utf16-v1`

#### UTF-16 strings (opt-in)

Select `"languageProfile":"checked-f64-utf16-v1"` for binary64 numbers,
undefined and UTF-16 string values, including lone surrogate code units:

```ts
export function main(): void {
  const text = "😀";
  console.log(text.length);                    // 2
  console.log(text.charCodeAt(0));              // 55357
  console.log(JSON.stringify(text.charAt(0)));  // "\ud83d"
  console.log("\ud83d" + "\ude00" === text);     // 1
  console.log(JSON.stringify(varta.json.parse<string>('"\\ud800"')));
  // "\ud800"
}
```

Length and character methods count UTF-16 code units. `charAt` returns an empty
string outside the range; `charCodeAt` returns NaN. Positions accept numbers,
truncate toward zero and default to zero; implicit argument coercion is unsupported.
Content comparisons use unsigned UTF-16 code-unit order. JSON escapes unpaired
surrogates and checked parsing preserves escaped or raw lone units in string values.

Host ABI v4/value ABI v5 and JSON schema v4 keep this representation separate
from every UTF-8 profile. Files, environment values and HTTP response bodies are
validated as UTF-8 and converted to UTF-16. Resource names must convert exactly:
lone surrogates and NUL fail before OS access, without replacement or normalization.
Diagnostic logs replace unpaired units with U+FFFD; use JSON for lossless inspection.

The 1 MiB string limit counts stored bytes (at most 524288 code units), and the
64 KiB static budget also counts UTF-16 bytes. JSON input has a 64 KiB physical
and converted-text limit; output has both UTF-8 JSON and stored UTF-16 limits.
Closed record keys/schema metadata remain valid UTF-8; surrogate keys, string
bracket access, dynamic unions and Tool/HTTP adapters remain unsupported.

### `checked-f64-utf16-v2`

#### Primitive unions (opt-in)

Select `"languageProfile":"checked-f64-utf16-v2"` for unions of `number`, `string`,
`boolean`, `null` and `undefined`, including parameters, returns, mutable bindings,
record fields and explicitly typed array elements:

```ts
function label(value: number | string | undefined): string {
  if (typeof value === "number") { return JSON.stringify(value + 0.5); }
  return typeof value === "string" ? value + "!" : "missing";
}
export function main(): void {
  let value: number | string | undefined = 1.5;
  console.log(label(value)); // 2
  value = "😀";
  console.log(label(value)); // 😀!
  const values: (number | string | undefined)[] = [value, undefined, -0];
  console.log(JSON.stringify(values)); // ["😀",null,0]
}
```

Runtime `typeof`, strict equality, truthiness and union `&&` / `||` / `??` preserve
actual values and short-circuit effects. Mixed primitive ternaries infer a union.
Direct-local `typeof`, null and global-undefined strict guards narrow `if` and
ternary branches. Narrowing stays inside the guarded branch; writes to its refined
binding are currently rejected, including loop back edges. Arithmetic and system
calls require the appropriate narrowed type. Other loose/coercive union comparisons,
object/array union members, `any`/`unknown`, assertions and uninitialized bindings
remain unsupported. Use explicit union annotations for mixed array elements.

Host ABI v5/value ABI v6 stores immutable tagged primitive cells in the traced
heap. Numeric payloads retain all bits; only string payloads create a child edge.
JSON omits union fields whose current value is undefined, nulls such array entries,
and returns a real `string | undefined` for an undefined-capable union root.
Schema v5 retains strict parsing of supported non-union targets; parsing a union
or a container with union fields is explicitly rejected. Tool/HTTP adapters remain
pending. All earlier profiles and the default i32 profile keep their behavior.

### `checked-f64-utf16-v3`

#### Optional fields and missing values (opt-in)

Select `"languageProfile":"checked-f64-utf16-v3"` for optional primitive fields,
own-property checks and missing reads:

```ts
export function main(): void {
  const value: { label?: string; n: number } = { n: 1.5 };
  console.log(Object.hasOwn(value, "label")); // 0
  console.log(value.label);                  // undefined
  value.label = "😀";
  console.log(JSON.stringify(value));        // {"n":1.5,"label":"😀"}
  value.label = undefined;
  console.log(Object.hasOwn(value, "label")); // 1
  console.log(JSON.stringify(value));        // {"n":1.5}
  const absent = [1.5][9];
  console.log(absent ?? "missing");          // missing
  console.log(JSON.stringify("😀"[0]));       // "\ud83d"
  let pending: string | undefined;
  console.log(pending);                      // undefined
}
```

An absent field differs from an own property whose value is undefined. New fields
first assigned after construction join insertion order; overwriting them keeps
their place, and integer-index keys still enumerate first. `Object.hasOwn` accepts
closed records and literal keys in this profile; v12+ adds dynamic String own-key
lookup as specified above. Optional field values must be primitives/unions;
readonly fields, deletion, arbitrary new keys and prototype lookup remain unsupported.
Reads of statically absent literal record keys return undefined, except inherited
prototype names, which are rejected. Optional inherited names are also rejected.

Numeric primitive-array and UTF-16 string reads return `T | undefined`. Indices
must be exact nonnegative integers within the actual length; other numeric values
return undefined without truncation. Minus zero selects index zero. Array writes
still require an in-range index; resizing/sparse writes and object/array-element
reads that need reference unions remain unsupported. String/number key coercion
is not supplied. Earlier profiles keep their existing rejection/trapping behavior.

An annotated `let` without an initializer is admitted only when its type includes
undefined; declaration-before-use checks remain. Undefined-capable function returns
may be bare or fall through. Host ABI v6/value ABI v7/schema v6 separate this
behavior and the presence/order record trailer from all older artifacts. Strict
JSON parsing still rejects optional/union targets, while required record parsing
produces the new layout. Tool/HTTP adapters remain pending.

### `checked-f64-utf16-v4`

#### Reference unions (opt-in)

Select `"languageProfile":"checked-f64-utf16-v4"` to combine primitives with a
closed record shape and an array element type:

```ts
function label(value: { n: number } | number[] | undefined): string {
  if (Array.isArray(value)) { return JSON.stringify(value); }
  return typeof value === "object" ? JSON.stringify(value.n) : "missing";
}
export function main(): void {
  const object = { n: 1.5 };
  const values = [object];
  const found = values[0];
  if (found !== undefined) { found.n = 2.5; }
  console.log(JSON.stringify(object)); // {"n":2.5}
  console.log(found === object);       // 1
  console.log(label([1.5, 2.5]));       // [1.5,2.5]
  console.log(values[9]);              // undefined
}
```

Nullable indexed reads now work for arrays of objects and arrays. Optional record
fields may also contain these references. Boxing retains the existing object;
mutation through an alias is visible and strict equality compares identity.
Objects/arrays are truthy even when empty. typeof reports object for both, including
null; exclude null separately when present. Direct-local `Array.isArray` and its
negation narrow if/ternary branches. Contextual literals, primitive subsets and
short-circuit operations retain the same evaluation order and owned references.

The checked increment permits **one record shape and one array element type per
union**, plus primitives/null/undefined. Multiple record shapes or array element
types, assertions and writes to guard-refined bindings remain rejected. JSON
serializes nested references with cycle/depth/node/byte checks; diagnostic union
logs use JSON for record/array values. Strict typed JSON still accepts only
non-union/non-optional targets; parsed required records use the new layout.
Tool/HTTP adapters remain pending.

Host ABI v7/value ABI v8/schema v7 uses reference-union tag 9 and versioned pure
imports. Primitive tag 8 retains its old meaning. Reference cells preserve exact
GC edges and never trace numeric bits. Earlier profiles keep their behavior.

### `checked-f64-utf16-v5`

#### Typed JSON with optional fields and unions (opt-in)

Select `"languageProfile":"checked-f64-utf16-v5"` to decode a closed schema with
optional fields and mixed values:

```ts
export function main(): void {
  const value = varta.json.parse<{
    child?: { n: number };
    items: (number | string | null)[];
    label?: string;
  }>('{"items":[1.25,"😀",null],"child":{"n":2.5}}');
  console.log(Object.hasOwn(value, "label")); // 0: absent
  const child = value.child;
  if (child !== undefined) { child.n = 3.5; }
  console.log(JSON.stringify(value));
  // {"child":{"n":3.5},"items":[1.25,"😀",null]}
}
```

All input and schema checks finish before guest allocation. Each union selects
its unique JSON-kind member, without coercion or speculative object-shape matching.
The existing limit of one record shape and one array element type per union stays.
Missing optional fields read as undefined but remain absent; later assignment
sets presence normally. Required fields must be supplied even when their type
includes undefined. JSON has no undefined token. Duplicate/unknown keys, wrong
members, missing required fields, non-finite numbers and budget violations trap.
UTF-16 lone units and signed zero survive decoding; record enumeration is normalized
to schema order, with absent fields excluded until assigned.

Host ABI v8 introduces `json_parse_v9` and JSON schema v8 while retaining value
ABI v8 and `json_stringify_v8`. Earlier profiles retain their parsing rules.
This remains the strict typed API; generic JSON.parse, revivers, broader shapes,
resizable arrays and new-profile Tool/HTTP adapters remain pending.

### `checked-f64-utf16-v6`

#### Resizable arrays (opt-in)

Select `"languageProfile":"checked-f64-utf16-v6"` for dense arrays whose length
can change through `push` and `pop`:

```ts
export function main(): void {
  const values: (number | string)[] = [];
  const alias = values;
  console.log(values.push(1.25, "😀")); // 2
  console.log(alias.pop());            // 😀
  console.log(JSON.stringify(values)); // [1.25]
  console.log(alias === values);       // 1
  alias.pop();
  console.log(values.pop());           // undefined
}
```

Array identity stays stable while backing storage grows geometrically. Aliases,
record fields, function/module boundaries and parsed JSON arrays see the same
updates. All push arguments evaluate once from left to right before appending;
`push()` returns the current length. `pop()` removes the last element and returns
its actual value, or undefined when empty. Removed references are cleared, while
returned objects remain alive through ordinary roots.

The current bound remains **1024 elements**. A push exceeding it traps before
changing the target, after argument evaluation. Indexed writes still replace
existing elements only; assigning length, holes, deletion, spreads, constructors,
iterators and array methods beyond push/pop/at/slice/includes/indexOf remain unsupported. Closed
element/union types and conservative guard narrowing still apply.

`at(index?)` supports negative indices from the end and returns the element or
undefined when outside the array. `slice(start?, end?)` creates a fresh array with
independent storage and the same element type. Object/array elements keep their
original references, so changing a nested object is visible through both arrays.

```ts
export function main(): void {
  const values = [{ n: 1.25 }, { n: 2.5 }];
  const copied = values.slice(-1);
  const last = copied.at(-1);
  if (last !== undefined) { last.n = 3.5; }
  copied.pop();
  console.log(JSON.stringify(values)); // [{"n":1.25},{"n":3.5}]
  console.log(copied.length);          // 0
}
```

Bounds accept number, undefined or number|undefined. Fractions truncate toward
zero, NaN acts as zero, and infinities are handled without integer conversion
traps. Slice clamps negative/outside bounds; undefined end selects the current
length. Receiver/start/end expressions evaluate once before selecting the range,
so their mutations use the current array storage. Boolean/string/null coercion,
spread arguments and borrowed methods remain unsupported. These are additive
compiler operations within the same v6 profile and unchanged ABI/layout.

`includes(value?, fromIndex?)` returns a boolean; `indexOf(value?, fromIndex?)`
returns the first matching index or -1. Search values may be any supported checked
value; different kinds do not match. Strings compare UTF-16 contents and objects/
arrays compare identity. Includes finds NaN, while indexOf does not; both match
the two signed zeroes. Omitting the value searches for undefined.

```ts
export function main(): void {
  const values = [0 / 0, -0, 2.5];
  console.log(values.includes(0 / 0)); // 1
  console.log(values.indexOf(0 / 0));  // -1
  console.log(values.indexOf(0));      // 1
  console.log(values.includes(2.5, -1)); // 1
}
```

The optional starting index follows the numeric/undefined bound rules above.
Receiver, value and starting-index expressions execute once before the scan,
even when the array is empty. Search itself allocates nothing and preserves status.

Host ABI v9/value ABI v9/schema v9 isolate the new storage format. JSON uses
`json_stringify_v9`/`json_parse_v10`; earlier profiles retain fixed-length arrays.
Tool/HTTP adapters for the newer profiles remain pending.

### `checked-f64-utf16-v7`

#### Primitive exceptions (opt-in)

Select `"languageProfile":"checked-f64-utf16-v7"` for explicit `throw` and
`try/catch/finally`, including exceptions crossing named function/module calls:

```ts
function checked(value: number): number {
  if (value < 0) { throw "negative value"; }
  return value;
}
export function main(): void {
  try {
    console.log(checked(-1));
  } catch (error) {
    if (typeof error === "string") { console.log(error); }
  }
  console.log("continued");
}
```

Throw supports Number, boolean, string, null, undefined and their primitive
unions. A catch binding has that checked primitive union; narrow it before
arithmetic/string operations. `catch { ... }`, nested handlers, rethrow, return,
break and continue are supported. Unfinished expressions and later argument/
system effects are skipped. Caught values retain exact bits/code units and remain
alive through collection. A pure catch does not reset the last system status.

`finally` runs once after normal completion, return, throw, break or continue
leaving its protected region. Return values are saved before finalization; a new
abrupt completion in the finalizer replaces the old one. Nested finalizers run
from inside out, retaining saved values through collection.

This increment rejects Error objects, aggregate throws, catch
annotations/destructuring and general unknown/any values. JSON validation failures,
index/resource traps and authorized process exit remain execution boundaries;
they bypass both catch and finally.
Capability denial still uses the existing status/result API; application code may
explicitly throw after inspecting it.

Host ABI v10 keeps value/schema v9 and adds a checked, rooted exception channel.
Uncaught values become an owned typed `RuntimeError::Uncaught`; rendered string
diagnostics are escaped and limited to 256 UTF-16 units. Native and portable
artifacts use the same behavior. Earlier profiles retain their original contracts.

### `checked-f64-utf16-v8`

#### Error objects (opt-in)

Select `"languageProfile":"checked-f64-utf16-v8"` for the first checked Error
model, inheriting v7 exceptions/finally and the preceding value features:

```ts
function fail(): void { throw new Error("invalid input"); }
export function main(): void {
  try { fail(); }
  catch (error) {
    if (error instanceof Error) { console.log(error.message); }
  }
}
```

`Error(message?)` and `new Error(message?)` accept string, undefined or their
union, evaluating the argument once. Each call creates a fresh object with an
immutable `name` of `"Error"` and checked UTF-16 `message`. `toString()` and logging
use `Error` or `Error: message`. `Object.hasOwn(error,"message")` distinguishes
an omitted/undefined argument from an explicit empty string. The message is
non-enumerable, so JSON.stringify emits `{}`. Arrays, records, optional fields,
unions and function calls retain Error identity and references through collection.
Catch values include Error and primitives; narrow before reading properties.

Host ABI v11/value ABI v10/schema v10 reject earlier interpretations of these
objects. Uncaught errors expose an owned typed message and presence flag, with
bounded escaped diagnostics. Earlier profiles retain their behavior.

This increment rejects property mutation, coercive message arguments, causes,
subclasses/native error families, prototype editing and stack traces. Typed JSON
parsing cannot construct Error objects. Resource/JSON/index traps and authorized
process exit still bypass catch/finally; system failures retain result/status APIs.

### `checked-f64-utf16-v9`

#### Native error families (opt-in)

Select `"languageProfile":"checked-f64-utf16-v9"` for `TypeError`, `RangeError`
and `SyntaxError`, with or without `new`. Each accepts the checked string/undefined
message rules of Error. An instance matches its own family and `instanceof Error`,
while siblings remain distinct. Its immutable `name`, `toString()`, logging and
uncaught diagnostics preserve the family.

```ts
function fail(): void { throw new TypeError("invalid input"); }
export function main(): void {
  try { fail(); }
  catch (error) {
    if (error instanceof TypeError) { console.log(error.message); }
  }
}
```

In this profile an `Error` annotation accepts all four supported classes; specific
family annotations and closed family unions retain their restrictions. Guard a
base Error before passing it to a TypeError parameter. Arrays, records, calls,
rethrow, finally and collection preserve identity and class. JSON still emits `{}`.
Host ABI v12/value ABI v11/schema v11 keep earlier profiles unchanged.

These constructors currently create explicit guest exceptions. Existing checked
JSON/index failures and execution limits remain fatal; they are not automatically
converted into these classes. Other native families, custom subclasses, writable
properties, cause/options and stack traces remain pending.

#### String repetition (opt-in)

`checked-f64-utf16-v9` and later additionally support `String.repeat(count?)`:

```ts
export function main(): void {
  console.log("ab".repeat(3)); // ababab
  try { "x".repeat(-1); }
  catch (error) {
    if (error instanceof RangeError) { console.log(error.message); }
  }
}
```

Counts accept Number, undefined and their union. Omitted/undefined/NaN counts
produce an empty string; fractions truncate toward zero. A negative normalized
count or either infinity raises RangeError, including on an empty receiver.
UTF-16 code units, lone surrogates and embedded NULs are copied exactly. Receiver
and count run once in source order, and live values survive collection.

The operation uses one result allocation and bulk-copy doubling for nonempty
results. It adds no host import, capability, ABI change or compiler helper.
Existing string/memory/fuel/deadline limits remain fatal; count coercion, borrowed
methods and non-string receivers remain unsupported.

### `checked-f64-utf16-v10`

#### Recoverable checked JSON errors (opt-in)

Select `"languageProfile":"checked-f64-utf16-v10"` to catch failures of
`varta.json.parse<T>`:

```ts
export function main(): void {
  try {
    const value = varta.json.parse<{n: number}>('{"n":');
    console.log(value.n);
  } catch (error) {
    if (error instanceof SyntaxError) { console.log(error.message); }
    else if (error instanceof TypeError) { console.log(error.message); }
  }
}
```

Malformed JSON produces SyntaxError. Valid JSON that violates the closed schema
(including unknown/duplicate/missing fields or unsupported numeric values) produces
TypeError. This is Varta's checked API, not unrestricted JavaScript JSON.parse.
Messages are fixed and do not echo input. Calls, rethrow and finally use the same
owned Error model; failed parsing stops later expression/argument effects.

Input/transcoding size, array/node/depth/string limits, fuel, deadlines, memory,
root/output/host-call budgets, invalid ABI/schema/helpers and authorized exit
remain fatal. Only declared syntax/type failures enter catch. System status and
capability grants retain their meaning. The opt-in requires host ABI v13 and
json_parse_v13, retaining value/schema v11 and existing serializers/value imports.
Earlier profiles retain fatal JSON failures. General JSON.parse, serializers and
other API error conversions remain further core work.

### `checked-f64-utf16-v11`

#### Typed function values (opt-in)

Select `"languageProfile":"checked-f64-utf16-v11"` for typed function values:

```ts
function apply(fn: (value: number) => number, value: number): number {
  return fn(value);
}
export function main(): void {
  console.log(apply(value => value + 1, 41)); // 42
}
```

Functions can be stored, reassigned, passed and returned with exact parameter and
result types. Named function references retain their identity across import aliases;
each arrow evaluation creates a fresh identity. Arrow parameters can use the
expected function type, and expression/block bodies are supported. Local calls
preserve argument order, errors, permissions and resource limits.

This increment supports arrows without captured outer variables. Capturing locals,
function expressions, function-valued object/array fields, optional/rest/default
parameters, async functions and generic functions remain unsupported. Functions
cannot enter JSON, unions or the exception channel. Use explicit annotations for
block-arrow results other than the existing default Number/void inference. The
profile uses host ABI v14 with unchanged value/schema v11, and inherits v10 APIs;
modern Tool/HTTP adapters remain pending. See the [function contract](architecture.md#checked-f64-utf16-v11--typed-function-values).

### `checked-f64-utf16-v12`

#### Lexical closures (opt-in)

Select `"languageProfile":"checked-f64-utf16-v12"` for lexical captures:

```ts
function counter(value: number): () => number {
  return () => { value = value + 1; return value; };
}
export function main(): void {
  const next = counter(0);
  console.log(next()); // 1
  console.log(next()); // 2
}
```

Closures share live bindings with their outer scope and sibling closures. Separate
calls own separate bindings. Nested closures can forward captures, and escaped
strings, objects, arrays, function values and captured host-result pairs stay
rooted. Each iteration's captured `let` binding is separate, including `continue`.
Managed tracing also reclaims unreachable cycles involving closures and bindings.

This increment captures bindings initialized before arrow creation. Captures
before initialization (including self-initialization), mutable captured union
narrowing and captures under an active guard refinement are explicitly rejected. These older-profile restrictions are relaxed by v24
write-aware refinement/capture analysis.
Function expressions, private self names and typed this are now available below.
Outer-binding recursive initialization, function-valued aggregates/JSON and async
remain pending. The profile uses host ABI v15, unchanged value/schema v11,
and retains all preceding system permissions and fatal execution limits. See the
[closure contract](architecture.md#checked-f64-utf16-v12--lexical-closures).

#### String ranges (v12 and later)

String.slice(start?, end?) and String.substring(start?, end?) accept Number,
undefined or their union, with a narrowed string receiver:

```ts
export function main(): void {
  console.log("abcdef".slice(-3, -1));     // de
  console.log("abcdef".substring(4, 1));  // bcd
  console.log(JSON.stringify("A😀B".slice(1, 2))); // "\ud83d"
}
```

Both methods index UTF-16 code units, including lone surrogates and embedded NULs.
Omitted/undefined start is zero; omitted/undefined end is length. NaN becomes zero
and fractions truncate toward zero. Slice resolves negative bounds relative to
length and returns empty for a reversed range. Substring clamps negative bounds
to zero and swaps reversed bounds. Infinities and huge finite values are clamped
before integer conversion.

The receiver, start and end evaluate once in order, including empty results.
Nested calls and later bound mutations retain earlier operand values. Nonempty
results use one owned allocation and a bounded bulk copy, never an interior
pointer into the source; empty results use the canonical empty string. Existing
roots preserve receivers/results, saved returns and physical pairs through GC and
finally. Source and result byte, memory, fuel and deadline bounds remain active;
exhaustion stays fatal. The maximum-byte tests use an explicit test fuel budget,
without increasing runtime defaults.

No coercion, borrowed/computed methods, generic/spread calls, extra arguments or
nullable receiver without narrowing are admitted. Existing Array.slice dispatch
and earlier profiles retain their behavior. Private Wasm helpers add no host
operation, capability, helper export or ABI/layout change. See the
[contract](architecture.md#string-range-contract).

#### String search (v12 and later)

String.includes, startsWith and endsWith accept a required string needle and an
optional Number/Undefined position, including that union, on a narrowed string:

```ts
export function main(): void {
  console.log("A😀B".includes("\ude00", 2)); // 1: exact UTF-16 unit
  console.log("abcdef".startsWith("cd", 2)); // 1
  console.log("abcdef".endsWith("cd", 4));   // 1: end position is exclusive
  console.log("abc".includes("", Infinity)); // 1
}
```

Includes searches at or after the position; startsWith tests exactly there.
EndsWith tests the suffix ending at its position. Omitted/undefined positions
are zero for includes/startsWith and length for endsWith. NaN becomes zero,
fractions truncate toward zero, and all positions clamp to 0..length before integer
conversion. Negative positions select zero rather than counting from the end.
An empty needle always matches after argument effects. Matching is case-sensitive,
uses UTF-16 code units and preserves lone surrogates/NULs without normalization.

Receiver, needle and position evaluate once in order. Private Wasm helpers read
existing strings without allocating or calling the host; boolean results are
canonical. Roots preserve earlier operands across argument effects and GC, while
throws skip later arguments. Array.includes keeps its existing SameValueZero,
hole and relative-position behavior. Earlier profiles remain unchanged.

The initial includes implementation scans candidate positions and may exhaust
execution fuel on large repetitive inputs. Existing fuel/deadline limits stop it
fatally; they do not become catchable errors. Maximum-byte tests use an explicit
test fuel budget without increasing runtime defaults. Regex/coercive needles,
omitted needles, borrowed/computed methods, generic/spread calls, extra arguments
and nullable receivers without narrowing remain unsupported. No host operation,
capability, helper export or ABI/layout change. See the
[contract](architecture.md#string-search-contract).

#### Array callbacks (v12)

`checked-f64-utf16-v12` supports `map`, `filter`, `forEach` and `find` through
ordinary typed functions and closures:

```ts
export function main(): void {
  const values = [1, 2, 3];
  const offset = 10;
  console.log(JSON.stringify(values.map(value => value + offset))); // [11,12,13]
  console.log(JSON.stringify(values.filter(value => value > 1))); // [2,3]
  console.log(values.find(value => value === 2)); // 2
  let sum = 0;
  values.forEach(value => { sum = sum + value; });
  console.log(sum); // 6
}
```

Fixed callbacks take zero to three parameters: a checked prefix of `(value, index,
array)`. Optional/default/rest forms are described below. The receiver and callback are evaluated once, then the initial length is
saved. Later appends beyond that length are ignored; replacements and regrowth
within it are read at their turn. Filter/forEach skip removed entries. Find visits
removed entries as undefined, so its callback value is checked as `T | undefined`;
use a guard before arithmetic/property access. Find returns the value read before
the callback, or undefined if no predicate succeeds. Map/filter return fresh arrays;
filter retains the value read before the predicate, including reference identity.

Map infers expression results and requires an array-supported value type; void
callbacks produce undefined entries. Filter/find apply checked truthiness; host
result pairs cannot serve as predicates. ForEach ignores callback results and
returns undefined. Unannotated inline callback blocks infer their result from
typed return sites, including supported unions, bare returns and possible
fallthrough to undefined. Explicit annotations remain checked. A block without a
value-returning site keeps void semantics. An optional thisArg is supported as
described below; borrowed/prototype methods remain unsupported.

Arrays remain dense in v12. If map encounters an index removed before its turn,
it fails with the existing fatal checked-index trap: sparse mapped results are
not implemented. It never silently compacts or substitutes undefined for a hole.
Recoverable callback exceptions stop iteration normally; resource/ABI traps and
authorized exit stay fatal. No ABI/import/permission changes are required. Existing
1024-element, function/code-size and execution bounds apply. See the
[array callback contract](architecture.md#additive-array-callback-operations).

#### Array reduce, some and every (v12 and later)

These methods use the same typed functions and closures, including in v14:

```ts
export function main(): void {
  const values = [1, 2, 3];
  console.log(values.reduce((total, value) => total + value, 0)); // 6
  console.log(values.reduce((total, value) => total + value)); // 6
  console.log(values.some(value => value > 2)); // true
  console.log(values.every(value => value > 0)); // true
  console.log(values.reduce((text, value) => text + "!", "seed")); // seed!!!
}
```

All three snapshot length after their argument expressions and inspect live
entries at each turn. They skip removed indices and, from v13, holes. Some stops
at the first truthy predicate, every at the first falsy predicate. Empty/all-hole
arrays return false for some and true for every without invoking the predicate.
Predicates support the same checked truthiness and fixed-parameter prefix as
filter, with optional/default/rest parameters and typed thisArg described below.

Reduce accepts a callback plus an optional initial value. Its fixed parameters
form a checked prefix of `(accumulator, value, index, array)`, and its result must have
the accumulator type. That type comes from the seed expression, or the element
type without a seed. Inline reducer arrows infer parameters and use that type
for expression/block returns. An annotated accumulator parameter, an annotated
inline return type, or a named/local/returned reducer signature provides context
for the seed, including empty arrays, nested literals and closed optional records.
Without that context, an aggregate seed can use the expected aggregate result
from a variable, return or argument position. Scalar seeds keep their own type
when only the surrounding result is wider. Explicit undefined is a supplied seed;
a void seed expression evaluates once and supplies undefined.

Without a seed the first present element initializes the accumulator without a
callback. No present element raises a catchable TypeError with message
`Reduce of empty array with no initial value`. With a seed an empty/all-hole
array returns it unchanged. Accumulators retain references, function values and
physical VartaResult pairs through allocations and exceptions. Existing limits,
capabilities, status and ABI versions are unchanged. See the
[reduction contract](architecture.md#additive-array-reduction-and-quantifiers).

For example, these empty seeds now receive their types from context:

```ts
export function main(): void {
  const collected: number[] = [1, 2].reduce((items, value) => {
    items.push(value);
    return items;
  }, []);
  const nested = [1, 2].reduce((state: { items: number[] }, value) => {
    state.items.push(value);
    return state;
  }, { items: [] });
  console.log(JSON.stringify(collected)); // [1,2]
  console.log(JSON.stringify(nested)); // {"items":[1,2]}
  console.log(JSON.stringify([1, 2].map(value => { return { n: value }; })));
}
```

Inference is limited to the existing checked types. Multiple incompatible record
shapes/array element types, function/physical-result unions and a seed with no
inferable type or context still require a different declaration or an explicit
annotation. Return analysis is conservative: all lowered return sites contribute,
including ones in finalizers; a possible normal exit contributes undefined.
Ordinary arrows outside these contextual array callbacks retain their earlier
Number/void defaults.

#### Anonymous functions and typed this (v12 and later)

Anonymous `function` expressions share the existing managed closures, exact
signatures and return inference. Parameter types come from annotations or a
function/Array callback context:

```ts
export function main(): void {
  const context = { offset: 10 };
  const add = function (this: { offset: number }, value: number) {
    return this.offset + value;
  };
  console.log(add.call(context, 2)); // 12
  console.log(JSON.stringify([1, 2].map(add, context))); // [11,12]

  const reader = function (this: { offset: number }): () => number {
    return () => this.offset;
  };
  const read = reader.call(context);
  context.offset = 20;
  console.log(read.call({ offset: 99 })); // 20: lexical this
}
```

A typed `this` parameter is separate from positional arguments. In this increment,
read `this` through an explicit function annotation or a function-type
context containing `this`. Calls retain primitive/null/undefined values exactly;
there is no global-object substitution or primitive boxing. Bare calls supply
undefined and require a receiver type admitting it. `this: void` uses undefined.

Map/filter/forEach/find/some/every accept an optional second argument as thisArg.
Receiver, callback and thisArg evaluate once in order before the length snapshot,
including empty arrays. Arrows and functions with no typed receiver ignore thisArg
while preserving its effects; arrows capture a surrounding typed this. Reduce's
second argument remains its seed, with undefined as the callback's this value.
`fn.call(thisArg, ...arguments)` is a checked intrinsic with exact argument types
and counts; call with no arguments supplies undefined to a zero-arity function.

Named expressions and declaration this are implemented below. Implicit this
inference from an object method/thisArg alone, top-level this, constructors,
arguments, bind/apply, async/generator functions and function-valued aggregates
remain unsupported. Optional/default/rest parameters are supported below. Arrow conversion to a dynamic-this function type
is also rejected. Explicit this is part of the invariant function type and counts
toward the physical parameter limit together with the closure environment.
No host operation, capability, heap layout or ABI version changes. See the
[typed this contract](architecture.md#anonymous-function-expressions-and-typed-this).

#### Named function expressions and declaration receivers (v12 and later)

A named expression owns a private immutable self name. Recursion follows that
same function object even after another variable is reassigned:

```ts
function add(this: { offset: number }, value: number): number {
  return this.offset + value;
}
export function main(): void {
  const factorial = function fact(n: number): number {
    if (n <= 1) return 1;
    return n * fact(n - 1);
  };
  console.log(factorial(5)); // 120
  console.log(add.call({ offset: 10 }, 2)); // 12
  console.log(JSON.stringify([1, 2].map(add, { offset: 10 }))); // [11,12]
}
```

Named expressions require a return annotation or an expected function type,
including reduce's accumulator context. Their name is visible only inside the
function and nested closures; parameters/body locals may shadow it. Assignment
to the private name is rejected. Escaped closures retain its original identity
and captured environment. Capturing an uninitialized outer variable still fails:
use the private name for recursion instead of the surrounding variable.

Declaration this remains separate from positional parameters and part of the
exact type. References/import aliases preserve the declaration's canonical
identity; call uses an explicit checked receiver, while a bare call supplies
undefined and requires a receiver type accepting it. A declaration receiver can
be captured lexically by returned arrows. Main, Tool Contract v1 and HTTP v1 keep
their existing receiver-free entry contracts. No ABI, import or heap-layout change
is required. See the [self-binding contract](architecture.md#named-expression-self-bindings-and-declaration-receivers).

#### Optional, default and rest parameters (v12 and later)

Declarations, arrows, function expressions, typed function values and call support
these parameter forms:

```ts
function sum(first: number = 1, ...values: number[]): number {
  return values.reduce((total, value) => total + value, first);
}
function optional(value?: number): number | undefined { return value; }
export function main(): void {
  console.log(sum()); // 1
  console.log(sum(undefined, 2, 3)); // 6
  console.log(optional()); // undefined
  const greet = function (name = "world") { return name; };
  console.log(greet()); // world
  console.log(JSON.stringify([1, 2].map((value, index, array, extra = 10) => value + extra)));
  // [11,12]
}
```

An optional parameter has type `T | undefined` in the body. A default runs only
for omission or explicit undefined, preserving null/zero/false/NaN as ordinary
values when admitted by the type. All supplied argument expressions run first;
initializers then execute left to right on every call. They can use initialized
earlier parameters, outer bindings, typed this and a named expression's self name.
Self/later parameter reads and captures before initialization are rejected;
function-body declarations are not visible to initializers.

Types come from annotations/function context, or scalar literal defaults
(number/string/boolean/null, including parenthesized/negative numbers). Other
default expressions require an annotation or context. Function/physical-result
optional/default parameters remain unsupported because their undefined unions
are not part of the checked value model. Required function/result-pair parameters
still work beside defaults.

A default before a later required parameter does not allow omission of that
required position: use an explicit undefined argument. A rest parameter must be
last and have a checked array type. Surplus arguments form a fresh dense array
of at most 1024 elements; empty rest is a fresh empty array. Children retain their
ordinary identity. Spread calls and parameter destructuring are supported below.

Function types retain exact optional/rest modes; parameter variance/adaptation
remains unsupported. Array callbacks receive their real value/index/array (and
reduce accumulator) arguments: fixed prefixes discard unused tails, optional
extra parameters get undefined/defaults, and rest captures all remaining arguments
with checked element types. A rest callback must admit the heterogeneous array
argument if it captures it. Empty traversals do not run defaults. For a named
reducer with an optional/rest first parameter, result type supplies the seed
context so an undefined-capable parameter does not widen a numeric accumulator.
No host operation, helper, layout or ABI version changes. See the
[parameter contract](architecture.md#optional-default-and-rest-parameter-contract).

#### Spread call arguments (v12 and later)

Checked function calls accept narrowed arrays interleaved with explicit arguments:

```ts
function sum(first: number = 1, ...values: number[]): number {
  return values.reduce((total, value) => total + value, first);
}
export function main(): void {
  const values = [2, 3];
  console.log(sum(...values)); // 5
  console.log(sum(undefined, ...values, 4)); // 10
  console.log(sum(...[])); // 1
}
```

Declarations, function values/closures and `fn.call(explicitThis, ...values)` use
this path. The callee, receiver and argument operands evaluate once in source
order. Each spread takes a shallow snapshot before the next operand runs: later
array replacement/deletion/resizing cannot change its values, while mutations to
referenced children remain visible. Sparse holes become explicit undefined values;
they can trigger defaults, and rest arrays contain present entries for those holes.

The flattened argument count is capped at 1024. Exceeding that resource bound is
fatal; invalid dynamic arity or incompatible argument types throw an owned
TypeError before any default initializer or function body starts. Supplied
argument effects have already occurred. Checked types must match each destination
or widen to its declared union; undefined-capable spread elements can narrow to a
required type after an undefined check. Broader union discrimination/coercion is
not supported. Explicit function and physical-result arguments can coexist with
array spreads without entering an aggregate.

Only checked arrays are admitted. String/general iterable spread, spread thisArg,
spread to host/built-in/Array methods and array/object spread literals remain
unsupported. Parameter destructuring is supported below. A generated call wrapper and its evaluated
operand segments count toward the existing function/body/32 physical-slot limits.
The implementation adds no import, heap layout, helper or ABI version. See the
[spread contract](architecture.md#bounded-spread-call-contract).

#### Destructured function parameters (v12 and later)

Declarations, arrows, function expressions, callbacks and typed function values
accept nested object/array parameters, renaming, defaults, elisions and rest:

```ts
function describe(
  {name: label = "item", child: {n}, ...other}:
    {name?: string; child: {n: number}; enabled?: boolean},
  [first = 1, ...tail]: number[] = []
): void {
  console.log(label);
  console.log(n);
  console.log(first);
  console.log(JSON.stringify(tail));
  console.log(JSON.stringify(other));
}
export function main(): void {
  describe({child: {n: 3}, enabled: false}, [2, , 4]);
}
```

The incoming aggregate needs an annotation or function/callback context. Its
original type remains the call signature; destructured names are local bindings.
Defaults run only for undefined, in parameter/property/element order, after all
supplied arguments. Earlier bindings, this and private self are visible; own/later
reads or captures and duplicate names are rejected. Nested defaults infer their
present/default value types within the existing checked union model. Top-level
parameter defaults retain the annotation/context rules above.

Object patterns use closed records and static keys, including computed string/
number literals. Reads observe earlier default mutations. Object rest is a fresh
shallow record preserving present fields, absent versus explicit undefined, child
identity and the source's current enumeration order. Array patterns read live
elements in sequence; elisions advance, missing/holes yield undefined, and an
exhausted iterator stays exhausted even if a later default grows the source.
Array rest is fresh and dense. In sparse v13+, its element type admits undefined;
dense v12 keeps the source element type. Individual array bindings also admit
undefined unless a default removes it.

Nullish aggregate inputs throw owned TypeError before later bindings/body code.
Resource limits remain fatal. A function's parameter patterns are bounded to 1024
nodes and depth 64, alongside existing source/type/body/frame limits. Destructured
rest parameters and nested array rest patterns are supported; object rest must
end in an identifier. Dynamic keys, primitive boxing, string/custom iterables,
array-as-object patterns remain unsupported.
Local const/let and the narrower catch object patterns are supported below. No ABI, import, managed layout or helper changes. See the
[parameter pattern contract](architecture.md#parameter-destructuring-contract).

#### Local const and let destructuring (v12 and later)

Local lexical declarations use the same checked object/array binding mechanism
as parameters, including aliases, nested patterns, defaults, elisions and rest:

```ts
export function main(): void {
  const {name: label = "item", ...other}: {name?: string; enabled?: boolean} =
    {enabled: false};
  let [first = 0, ...tail]: number[] = [1, , 3];
  first = first + 1;
  console.log(label); // item
  console.log(first); // 2
  console.log(JSON.stringify(tail)); // [null,3]
  console.log(JSON.stringify(other)); // {"enabled":false}
}
```

The initializer runs once. Its type is inferred, or supplied by an aggregate
annotation on the whole pattern; empty arrays still need an element context.
All names in the enclosing block start uninitialized, so own/later reads and
captures are rejected even across declarators/statements. Names initialize left
to right and earlier leaves are visible in later defaults. Const prevents binding
reassignment, including through closures; referenced children remain mutable.
Let supports the existing checked assignments and shared capture cells. Nested
blocks restore shadowed bindings, and escaped closures retain their own activation.

Patterns also work in ordinary `for` initializers. For let, the first iteration
and subsequent updates use distinct captured cells; a closure created in the
initializer keeps its original bindings. Continue/finally follows the normal
update path, while break/return/throw leaves the loop scope. Const loop bindings
retain one environment. Array for-of loops use per-iteration environments as
described below; for-in remains unsupported.

The parameter rules for live array reads, persistent exhaustion, dense rest,
object rest presence/order/identity, nullish TypeError and fatal resource limits
also apply. Shared depth/node limits are 64/1024 per lexical declaration, alongside
existing source/body/frame limits. Initializers are required for destructuring.
Var/ambient and top-level patterns remain unsupported; checked assignment
statements and the narrower catch object patterns are supported below. No new
host operation, heap layout, helper or ABI version is introduced. See the
[lexical binding contract](architecture.md#local-lexical-destructuring-contract).

#### Destructuring assignment statements (v12 and later)

Existing initialized mutable locals/parameters can be updated with checked
object/array assignment patterns:

```ts
export function main(): void {
  let name = "old";
  let first = 0;
  let tail: (number | undefined)[] = [];
  const read = () => name;
  ({name = "item"} = {});
  [first = 1, ...tail] = [2, , 3];
  console.log(read()); // item
  console.log(first); // 2
  console.log(JSON.stringify(tail)); // [null,3]
}
```

The RHS evaluates once before any target write. Nested patterns, static aliases/
keys, defaults, elisions and object/array rest reuse the shared binding machinery.
Writes update the existing captured cells in source order. Repeated targets are
allowed; later defaults can read earlier writes or the target's previous value.
Earlier successful writes remain visible if a later default or checked read
throws. Const, uninitialized/unknown targets, incompatible types and writes under
an active union guard are rejected with source diagnostics.

Exact types and declared union widening are accepted. When an array/optional
field can yield undefined but a compatible target requires a present value, a
pure guest check throws owned TypeError `Invalid destructuring assignment value`
for undefined before writing that target. Defaults use the target type as context,
including empty aggregate/function/result defaults. Homogeneous array targets can
provide RHS element context; ambiguous nested/empty inputs still need a typed
source. Aggregate and array types remain invariant, without arbitrary union
narrowing/coercion. Nullish aggregate errors, live array reads/exhaustion, shallow
rest identity/presence/order and fatal resource behavior keep their contracts.

Standalone statements (including parentheses), expression results/chains and
ordinary for initializer/update statements are supported. Checked member targets
are supported below; dynamic record keys in patterns, catch and for-in patterns remain
unsupported. Shared pattern
limits also apply per assignment. No new import/helper/layout or ABI version is
introduced. See the [assignment contract](architecture.md#checked-destructuring-assignment-contract).

#### Destructuring assignment values (v12 and later)

A successful assignment expression returns its original RHS, preserving identity,
all source fields, array holes and the actual contextual source type:

```ts
export function main(): void {
  let n = 0;
  const source = {n: 2, extra: "kept"};
  const original = ({n} = source);
  console.log(original === source); // 1
  console.log(original.extra); // kept
  const values: number[] = [, 3];
  const same: number[] = ([n = 7] = values);
  console.log(same === values); // 1
  console.log(Object.hasOwn(same, 0)); // 0
  console.log(n); // 7
}
```

Use assignment values in local initializers, arguments, returns, member reads,
callbacks, admitted operators/conditionals and nested assignment chains. Each
RHS evaluates once; outer targets wait for inner assignments to succeed. Defaults
only update targets and do not fill source holes/properties. Expected aggregate
context flows into the RHS, so a declared result type is checked against the whole
source rather than only selected fields. Existing inference/type restrictions
still apply to ambiguous aggregate literals and mixed operators.

Earlier operands/arguments, the saved RHS and the returned value remain rooted
across defaults, collection and later argument evaluation. A failed traversal
publishes no success result, skips later enclosing work and preserves prior writes
through catch/finally. Existing mutability/type restrictions, resource
budgets, status and real grants remain active. No import/helper/layout/ABI change.
See the [value contract](architecture.md#destructuring-assignment-value-contract).

#### Member targets in destructuring assignment (v12 and later)

Assignment patterns can write checked record fields and numeric array elements,
including nested targets, defaults and final object/array rest:

```ts
export function main(): void {
  const output = {n: 0};
  const values: number[] = [];
  ({value: output.n = 7} = {});
  [values[2]] = [output.n]; // v14 permits indexed extension
  console.log(output.n); // 7
  console.log(JSON.stringify(values)); // [null,null,7]
}
```

Each receiver and array index evaluates once, in source order, before reading
that source property/element or executing its default. Rest targets evaluate
before copying. Nested targets wait until their containing source value is read.
Receivers/indices stay fixed and rooted across defaults, even if a default
reassigns the original variable or resizes the array; the write uses the saved
receiver and its current backing. Later references observe earlier writes.
An exception skips remaining work and preserves successful writes.

Const prevents rebinding the variable; its mutable fields/elements remain valid
targets. Exact types, union widening, checked Undefined narrowing and target-typed
defaults reuse ordinary checked writes. The assignment still returns its original
RHS. Only admitted static record keys and Number array indices are supported;
Error/result properties, unresolved nullable receivers and coercive properties
remain rejected. Empty/ambiguous RHS arrays may need an element annotation.

V12 replaces existing array elements; v13 can also fill holes within length.
V14 extends arrays with holes and admits `.length` targets, using the existing
RangeError for invalid lengths. Numeric index validation follows default/RHS
effects. Resource violations, including lengths above 1024, stay fatal.
Declaration/parameter patterns still bind names. No imports, layouts or ABI
versions change. See the [member contract](architecture.md#destructuring-member-target-contract).

#### Array for-of loops (v12 and later)

For-of reads the values of a checked array once in ascending index order. In
sparse profiles (v13+), holes are visited as undefined, so an inferred loop variable
has the array element type plus Undefined:

```ts
export function main(): void {
  const values: number[] = [1, , 3];
  for (const value of values) {
    if (typeof value === "number") {
      console.log(value); // 1, then 3
    } else {
      console.log("missing");
    }
  }
  const rows: {n?: number}[] = [{}, {n: 2}];
  for (const {n = 7} of rows) {
    console.log(n); // 7, then 2
  }
}
```

Use const/let declarations, existing mutable identifiers, checked member targets
or object/array assignment patterns. Lexical head names are in TDZ during source
evaluation, stay local to the loop, and receive fresh capture cells each iteration.
Const prevents rebinding; let permits checked mutation. Head patterns reuse defaults,
rest, source-order initialization and the shared pattern budgets. Empty arrays
need an independently typed source, and type annotations on the loop head are not
admitted by the TypeScript syntax.

Evaluate and root the source once; assigning its original variable does not
replace the array being traversed. Length, backing and values remain live:
push/extension may add later visits, shrinking may end the loop, and holes or
deleted elements yield undefined. No length snapshot or hole skipping is used.
The current value is obtained before an assignment head evaluates its receiver
and index, so those effects only change later reads. A required assignment target
rejects undefined with the existing checked TypeError; earlier writes remain.

Break/continue, nested loops/switches, return/throw and finally follow existing
cleanup rules. Owned nullish array sources throw TypeError before targets/body;
incompatible source types fail compilation. Pure traversal preserves system
status, while source/head/default/body host calls require normal grants and host
ceilings. Array limits, fuel/deadlines, memory bounds and authorized exit retain
their behavior. V12 dense arrays infer their declared element type; earlier
profiles reject for-of. For-in, for-await-of, var/using heads, strings and
custom iterables remain unsupported. No iterator object, import, helper, layout
or ABI version is added. See the [loop contract](architecture.md#array-for-of-contract).

#### Do-while loops (v12 and later)

Do-while executes the body before evaluating its condition. Even a false initial
condition permits one body execution:

```ts
export function main(): void {
  let n = 0;
  do {
    n = n + 1;
    try {
      if (n < 3) { continue; }
    } finally {
      console.log(n); // 1, 2, 3
    }
  } while (n < 3);
}
```

Continue runs applicable finalizers and then tests the condition. Break, return
and throw skip the condition; a finalizer can replace those completions using the
existing rules. Nested loops/switches keep their own targets. Body-block names
remain local to that block; the trailing condition cannot read them. Captured
body bindings receive fresh cells on each body execution, while outer variables
retain their shared cells.

The condition uses the existing checked truthiness rules. Its effects execute
once per completed body/continue and may throw recoverable errors. Temporary
roots are cleared before the next iteration. Unconditional first-body returns/
throws are recognized by function completion analysis, including callback return
inference; possible break/continue paths still require a return afterward for
non-void functions. Body and condition effects retain real grants, status, memory,
fuel, host-call and deadline bounds. Authorized exit/resource traps remain fatal.
Earlier profiles reject do-while. Labels are supported below. No import, helper,
layout or ABI version changes. See the [contract](architecture.md#do-while-contract).

#### Labels and named branches (v12 and later)

Labels let a branch leave or continue an enclosing loop directly:

```ts
export function main(): void {
  outer: for (let n = 0; n < 3; n = n + 1) {
    for (const value of [1, 2]) {
      try {
        if (n < 2) { continue outer; }
        break outer;
      } finally {
        console.log(n); // 0, 1, 2
      }
    }
  }
}
```

Break may name any active supported labeled statement, including a block or
switch. Continue must name a while, do-while, for or array for-of loop; a chain
such as `a: b: for (...)` gives both labels the same iteration target. For continue
runs the update/per-iteration rebinding, while continue rechecks its test,
do-while continue checks its trailing condition, and for-of continue advances to
the next value. Unlabeled branches retain their nearest-loop/switch behavior.

Labels occupy a separate namespace from variables. Unknown labels, duplicate
active names, non-loop continue targets and jumps across function boundaries are
rejected with source diagnostics. A name can be reused after its labeled statement
ends. Labels count toward the existing nesting limit and do not create a lexical
scope; blocks/loops still provide their own scopes.

Crossed finalizers execute from inner to outer and may replace pending branches,
returns or throws. Roots are cleared for the scopes actually left, while escaped
captures and saved result pairs remain valid. Completion/callback analysis retains
named targets through inner loops and consumes them only at the matching label,
so an outer jump cannot falsely prove a guaranteed return. Host effects, status,
resource limits and fatal exit remain unchanged. Earlier profiles reject labels;
no ABI/import/helper/layout change. See the [contract](architecture.md#statement-label-contract).

#### Catch binding patterns (v12 and later)

A catch object pattern can extract name/message, rename bindings and initialize
defaults in source order. Both properties have type `string | undefined` because
Varta's exception channel admits Error families and primitive values:

```ts
export function main(): void {
  try {
    throw TypeError("invalid input");
  } catch ({name = "Unknown", message: detail = "no message"}) {
    console.log(name);   // TypeError
    console.log(detail); // invalid input
  } finally {
    console.log("finished");
  }
}
```

Error properties use the existing name/message contract, including inherited
empty message and native family names. Non-null primitive throws supply undefined
for these two properties; a default runs only for undefined. Null/undefined throws
cause an owned TypeError before defaults or the handler body, including for `catch
({})`. Errors in defaults escape to the outer handler and still execute applicable
finally blocks. Fatal execution limits and authorized exit continue to bypass them.

All bound names are declared before defaults, so self/later reads remain in TDZ;
initialized earlier names may be used by later defaults. Bindings are mutable,
scoped to the handler and receive fresh capture cells on each entry. Defaults
cannot see declarations inside the handler body. Type inference, exact roots,
saved returns/pairs, GC and named branches retain their existing contracts.

This increment supports identifier leaves and static name/message keys (including
literal computed keys), plus an empty object pattern. Array/rest patterns, other
properties, dynamic keys, catch type annotations and aggregate throws remain
unsupported; it does not infer a narrower caught type from the try body. Earlier
profiles retain identifier/omitted catch bindings. No host operation, helper export
or ABI/layout change. See the [contract](architecture.md#catch-binding-pattern-contract).

#### Dynamic own-property keys (v12 and later)

Object.hasOwn accepts a computed String key on a narrowed closed record. It tests
presence, including an explicitly stored undefined, without reading field values:

```ts
export function main(): void {
  const value: {name?: string} = {};
  const key = "name";
  console.log(Object.hasOwn(value, key)); // 0
  value.name = undefined;
  console.log(Object.hasOwn(value, key)); // 1
  console.log(Object.hasOwn(value, "missing" + "")); // 0
}
```

The record and key evaluate once in that order. Key effects can update presence
before lookup; reassignment of the original source variable does not replace the
captured receiver. Unknown names, including inherited names not declared as own
fields, return false. Required own fields named constructor/toString are checked
normally. No getter, prototype lookup or JSON serialization is performed. A true
result does not exclude Undefined from an optional field's value.

Comparison preserves exact UTF-16 units, NULs and case without normalization.
Record metadata names remain bounded UTF-8, so a lone-surrogate key cannot match.
String literals use the direct presence path, including false for unrepresentable
names or __proto__; this does not permit prototype initializers. Dynamic keys use
a balanced decision tree sorted in UTF-16 order, independently of UTF-8 shape
slot order, then read the existing presence bit. Receiver/key roots and earlier
operands/physical pairs survive calls, GC and finally cleanup.

Exactly two arguments are required. Dynamic Number/Boolean/Undefined/union keys,
coercion, symbols, unchecked/nullable receivers without narrowing, generic/spread
calls and shadowed Object remain unsupported. Existing record numeric-literal
keys and older profiles retain their rules.

V12+ also accepts computed String keys on narrowed Error families, including
Error-only unions: message tests its own presence bit, while name and all other
names return false. Omitted/undefined messages are absent; an explicit empty
String is present. V13+ admits String keys on arrays: length is always own, and
canonical decimal index names (0, 1, …) test presence within the current length.
Holes are absent; explicit Undefined is present. Leading zeros, signs, fractions,
exponents, NULs, lone surrogates and unknown/inherited names return false. Numeric
array keys retain their existing checks, including numeric -0 as index 0. Computed
String|Number union keys require narrowing. Both arguments run once in source
order, with the original receiver retained across key effects and GC.

No host operation,
helper export, managed layout or ABI change. All source/type/static-data/frame/root/
string/memory/fuel/deadline limits remain active. See the
[contract](architecture.md#dynamic-record-own-key-contract).

#### Computed record reads (v12 and later)

Closed records accept a computed, narrowed String key. The result combines every
declared field type with Undefined, so check it before using a specific type:

```ts
export function main(): void {
  const value: {name: string; score?: number} = {name: "Varta"};
  const key = "score";
  console.log(value[key]); // undefined
  value.score = 2.5;
  const found = value[key];
  if (typeof found === "number") { console.log(found + 1); } // 3.5
}
```

The record and key evaluate once in order. Lookup reads the original receiver
after key effects, including field mutations, and preserves shallow reference
identity and exact numeric bits. Missing and absent optional fields yield Undefined;
an explicit Undefined field remains own. Keys compare exact UTF-16 units, including
NULs, without case folding or normalization; lone-surrogate keys cannot match the
UTF-8 metadata names. Empty records still evaluate the key.

Prototype lookup remains unsupported. A computed inherited name such as toString,
constructor or __proto__ raises a catchable TypeError unless it names a declared
own field. Static inherited reads retain their compile-time rejection. No getters,
key coercion or prototype initialization are added. Dynamic Number/Boolean keys
remain rejected. Union keys and nullable sources require narrowing. Computed writes
are specified below. Literal String/Number accesses retain their exact field
types and earlier-profile rules. A result requiring multiple record shapes or
multiple array element types is rejected by the existing union limits.

The compiler reuses the balanced UTF-16 lookup tree and existing typed union
storage, roots and exception handling. All resource bounds remain active, with
no host operation, helper export or ABI/layout change. See the
[contract](architecture.md#computed-record-read-contract).

#### Computed record writes (v12 and later)

Simple assignment statements accept a narrowed String key on a narrowed closed
record. The selected field keeps its declared type:

```ts
function put(value: {name: string; score?: number}, key: string,
             item: string | number | undefined): void {
  value[key] = item;
}
export function main(): void {
  const value: {name: string; score?: number} = {name: "Varta"};
  put(value, "score", 2.5);
  console.log(JSON.stringify(value)); // {"name":"Varta","score":2.5}
  try { put(value, "name", 3); }
  catch (error) { console.log(error instanceof TypeError); } // 1
}
```

Receiver, key and right-hand side run once in that order. The receiver and key
are retained before RHS effects; reassignment of their original variables does
not redirect the store. After RHS success, an exact UTF-16 lookup selects a
declared field. A compatible value is stored with its Number bits/reference
identity intact. An absent optional field becomes present even for Undefined;
first insertion records order, while replacement keeps it. Integer-index names
still enumerate first numerically.

Unknown keys, including inherited names without declared own fields, throw
TypeError after RHS effects. An incompatible selected-field value also throws
TypeError without performing the requested store. Earlier RHS mutations/effects
are preserved. New properties, coercion, getters/setters and prototype access
remain unsupported. A value that cannot match any field is rejected at compile
time; an empty record still evaluates all three operands before failing at runtime.

Checks preserve full record shapes, invariant array element types and exact Error
families. No combined field union or erased type is invented: a source may have
several record shapes/array types when each store uses an admitted RHS type. An
aggregate literal receives context only if one corresponding shape/array type is
available; ambiguous empty literals require an explicitly typed value.

Standalone/parenthesized statements and ordinary for initializer/update positions
are supported. Computed assignment expressions/chains are specified below.
Arithmetic/logical assignments and numeric updates are specified below.
Destructuring targets, dynamic deletion, non-String keys and unnarrowed nullable
receivers remain unsupported. Literal accesses/writes and earlier profiles retain
their rules. All bounds, exact roots, exception/finalizer cleanup and grants/status
remain active, with no host operation, helper export or ABI/layout change. See the
[contract](architecture.md#computed-record-write-contract).

#### Computed record assignment values (v12 and later)

Computed record assignment can also be used as an expression. It returns the
original RHS value and type after the store succeeds:

```ts
export function main(): void {
  const left: {score?: number} = {};
  const right: {score?: number} = {};
  const key = "score";
  const result: number = (left[key] = right["score"] = 2.5);
  console.log(result + 1); // 3.5
  console.log(JSON.stringify(left));  // {"score":2.5}
  console.log(JSON.stringify(right)); // {"score":2.5}
}
```

Optional/union field storage does not widen the expression's original scalar,
record, array or Error result. Number bits and reference identity are retained.
A union RHS keeps its original union type; selecting a field at runtime does not
provide a static narrowing of the result. Context for aggregate literals follows
the existing target-field rules.

In a chain, receiver/key pairs evaluate once from left to right before the final
RHS; stores run from the innermost assignment outward. A failed inner write skips
outer stores and later arguments. A failed outer write keeps successful inner
stores and earlier effects. Reassigning source/key variables in an RHS cannot
redirect the prepared reference. Catch/finally and fatal-limit behavior remain
unchanged.

Supported expression positions include initializers, arguments, returns,
operators/conditional branches, callbacks, receivers/keys and for conditions.
Computed literal String/Number keys keep static field/type checks; unknown literal
keys remain compile errors. Dynamic String keys keep their runtime checks.
Local-variable, dot-property and array assignment values are specified below.
Error mutation remains unsupported. Dynamic compounds and numeric updates are
specified below. Existing assignment statements and destructuring expressions
keep their rules and may consume these new RHS values.

The compiler reuses rooted value blocks with a saved temporary floor to preserve
earlier operands, physical pairs and saved returns through nested assignments and
GC. No new host operation/import, helper export or ABI/layout change. See the
[contract](architecture.md#computed-record-assignment-value-contract).

#### Simple assignment values (v12 and later)

Local variables, declared record fields and numeric array elements also support
simple assignment expressions and mixed chains:

```ts
export function main(): void {
  let local: number | string = 0;
  const record: {score?: number} = {};
  const values: (number | undefined)[] = [undefined];
  const result: number = (local = record.score = values[0] = 2.5);
  console.log(result + 1); // 3.5
  console.log(JSON.stringify(record)); // {"score":2.5}
  console.log(JSON.stringify(values)); // [2.5]
}
```

The result keeps the original RHS type, bits and reference identity. Only the
stored copy is widened to an optional/union target. Local function values receive
their existing contextual signatures; local captured-result assignments preserve
both physical value/status slots. Constants, uninitialized bindings, writes under
guard refinement and incompatible record/array/function/result types remain
compile errors.

Receiver and numeric index run once before RHS effects. The store uses that
original receiver/index and its current backing after RHS success, even if the
RHS reassigns source variables or resizes the array. Numeric-domain checks and
bounds run after RHS in these expression paths. Indices must be finite integral
Numbers in 0..1023; invalid indices remain fatal. V12/v13 require a current in-range
element. V14 can extend arrays with intervening holes, retaining sparse presence,
the 1024-element ceiling and reference ownership.

V14 also admits `array.length = rhs` and literal `["length"]` expressions. Invalid
uint32 lengths raise RangeError; valid lengths above 1024 remain fatal. Shrink/
growth preserve existing clearing/hole rules. The expression returns the original
Number, including -0 when stored length becomes zero.

Prepared references and RHS values use the shared destructuring setters and rooted
value blocks. Mixed chains retain inner stores when an outer write fails, and
failures skip later stores/arguments. Earlier operands, saved returns, captured
cells, functions and result pairs survive nested calls, GC and finalizers. Error
objects/result fields remain immutable; coercive/String array indices, new record
properties, update operators and earlier-profile assignment expressions remain
unsupported. Arithmetic compound operators are specified below. No host operation/import, helper export or ABI/layout change.
See the [contract](architecture.md#simple-assignment-value-contract).

#### Arithmetic compound assignments (v12 and later)

The noncoercive operand rules below describe v12–v28. V29 extends primitive
conversion as described in [primitive compound assignment coercion](#primitive-compound-assignment-coercion-v29), while retaining checked storage.

Locals, declared static record fields and numeric array elements support `+=`,
`-=`, `*=`, `/=` and `%=` in statements and expressions. V14 also admits array
length. The expression returns the computed value stored in the target:

```ts
export function main(): void {
  let total = 1;
  const record = {count: 2};
  const values = [4];
  console.log(total += record.count *= values[0] += 3); // 15
  console.log(record.count); // 14
  console.log(values[0]); // 7
  let label = "Varta";
  console.log(label += "!"); // Varta!
}
```

Each receiver/index evaluates once. Its old value is read immediately, before RHS
effects; changing the same target in the RHS does not replace that saved value.
The computed result is stored through the prepared original reference and current
array backing. Nested compounds retain each outer old value while inner writes
complete. Earlier writes and RHS effects remain when a later operation fails.

Number/Number operands use existing binary64 arithmetic, including NaN/infinities,
signed zero and remainder. Only `+=` admits String/String concatenation, preserving
exact UTF-16 units and normal string limits. No coercion is added. Closed unions
and optional fields/array reads are checked after RHS success: matching Number
operands, or matching Strings for `+=`, are required. Incompatible kinds, absent
fields, explicit Undefined, holes and missing/noncanonical numeric reads raise
TypeError without the requested store. A statically impossible operand pair is a
compile error. Ordinary binary union expressions keep their narrowing rules.

Array reads follow the existing missing-value semantics before RHS evaluation.
After a successful arithmetic result, existing write checks use current storage:
v12/v13 writes beyond current length remain fatal; v14 may recreate/extend valid
indices with holes. V14 length stores retain RangeError for invalid uint32 results
and fatal results above 1024. Resource exhaustion remains fatal, including string
overflow, and skips catch/finally rather than becoming an operand error.

Constants, TDZ/guard-refined targets, String array indices,
Error/result mutation
remain unsupported. Original operand/result roots, earlier arguments/result pairs,
callbacks and saved returns survive GC/finalizers. The operation reuses binary
helpers, pure remainder, checked setters and value blocks; no ABI/layout, new host
operation or helper export is introduced. See the
[contract](architecture.md#arithmetic-compound-assignment-contract).

#### Dynamic record compound assignments (v12 and later)

The noncoercive operand rules below describe v12–v28. V29 extends primitive
conversion as described in [primitive compound assignment coercion](#primitive-compound-assignment-coercion-v29), while retaining checked storage.

Closed records also admit the five arithmetic compound operators with a narrowed
String key, in statements and expressions:

```ts
export function main(): void {
  const record = {count: 2, label: "Varta"};
  let key = "count";
  console.log(record[key] *= 3); // 6
  key = "label";
  console.log(record[key] += "!"); // Varta!
}
```

The receiver and exact UTF-16 key evaluate once, then the selected old value is
read before RHS effects. Reassigning the receiver/key or changing the same field
in the RHS cannot redirect the final write or replace that snapshot. Nested
assignments preserve completed inner writes if an outer operation fails.

This combines the existing computed-read and checked-write contracts. All field
types must fit the admitted read union: at most one record shape and one invariant
array element type, with no functions/result pairs. The RHS determines possible
Number/String arithmetic results; statically impossible cases fail compilation.
Runtime incompatible kinds, absent optional fields and unknown ordinary keys
raise TypeError after RHS effects without the requested write or property creation.
Unsupported inherited names fail during the old-value read, before RHS evaluation;
declared own fields with those names work normally. No coercion or prototype access
is added. Literal keys retain their exact static diagnostics and field types.

Computed results preserve Number bits and exact UTF-16 strings. The final store
checks the selected field's full declared type and keeps presence/insertion order.
Prepared references, old values and earlier operands/result pairs survive calls,
GC and finalizers. Existing permissions, status and fatal budgets are unchanged;
no ABI/import/helper export is added. See the
[contract](architecture.md#dynamic-record-compound-assignment-contract).

#### Logical assignments (v12 and later)

`&&=`, `||=` and `??=` support mutable initialized variables, declared record fields,
dynamic String record keys and numeric array indices. V14 also admits array length.
Each operator reads the old value once and runs the RHS and checked store only in
its selected branch:

| Operator | Run RHS and store when |
| --- | --- |
| `&&=` | The old value is truthy |
| `||=` | The old value is falsy |
| `??=` | The old value is Null or Undefined |

```ts
export function main(): void {
  let count: number | undefined = undefined;
  console.log(count ??= 5); // 5
  console.log(count ||= 9); // 5; RHS and store skipped
  console.log(count &&= 2); // 2
  const record: {n?: number} = {};
  console.log(record["n" + ""] &&= 3); // undefined
  console.log(Object.hasOwn(record, "n")); // 0; still absent
  console.log(record["n" + ""] ??= 7); // 7
}
```

Zero, negative zero, NaN, false, empty String, Null and Undefined are falsy;
records, arrays, functions and Errors are truthy. `??=` keeps zero, false and empty
String. No value coercion is added. The expression returns the original old value
on a skip, or the original RHS after a successful store. Result types combine
possible skipped members with the RHS, excluding only statically impossible
skipped kinds. A dynamic `&&=` read may therefore return Undefined even when all
declared fields are required.

Receiver and key/index effects run once before the read. The skipped branch runs
no RHS effects, write validation, payload changes or presence/order publication.
On the selected branch, RHS changes cannot redirect the saved reference; the
final array store uses current storage. V12/v13 retain current-length bounds;
v14 can recreate/extend valid numeric indices and keeps checked length errors.
Missing or invalid numeric reads can skip `&&=` without a write trap. Selected
invalid writes, resource limits and authorized exit retain their fatal behavior.

Static targets require a compatible RHS even when runtime would skip it. Dynamic
record keys retain read-union restrictions; selected stores check the complete
field type after RHS effects. Unknown ordinary keys can skip `&&=` as Undefined;
selected writes to them throw TypeError after RHS without creating a property.
Undeclared inherited names fail during the read before RHS. Prior effects and
successful inner writes survive a later failure. Exact roots/floors retain old/
new values, references, earlier operands/result pairs and saved returns through
GC, callbacks and finalizers.

Function bindings preserve signatures, arrow context and capture identity.
Constants, TDZ/guard-refined targets, result-pair targets, String array indices,
Error/result field mutation, destructuring logical targets and earlier
profiles remain unsupported. Existing truth/nullish tests, value blocks and checked
setters supply this feature; no new host operation, ABI/layout or helper export is
added. See the [contract](architecture.md#logical-assignment-contract).

#### Increment and decrement (v12 and later)

Prefix/postfix `++` and `--` support mutable initialized variables, declared record
fields, narrowed String record keys and numeric array indices. V14 also admits
array length. Both forms evaluate the reference and read its old value once:

```ts
export function main(): void {
  let count = 1;
  console.log(count++); // 1; count becomes 2
  console.log(++count); // 3; count becomes 3
  const record = {n: 4};
  const key = "n";
  console.log(record[key]--); // 4; record.n becomes 3
  const array = [5];
  console.log(--array[0]); // 4
  for (let i = 0; i < 2; i++) {
    console.log(i); // 0, then 1
  }
}
```

The old value must be Number. Static types with no Number member fail at the
target's source range; closed unions check the actual kind at runtime. Strings
(including `"1"`), Boolean, Null, Undefined, references and Errors are not coerced.
Invalid or missing operands raise TypeError without the requested write. Dynamic
record keys retain their bounded read-union/UTF-16 rules; inherited non-own names
fail during the read, and no update creates a property. Optional absence/explicit
Undefined remain distinct and unchanged by a failed update.

Compute old ± 1 using existing binary64 arithmetic, including NaN/infinities,
fractional values and large-number rounding. Store the new Number through the
prepared original reference using existing type/presence/bounds checks. Only after
success, prefix returns the new Number and postfix the original old Number,
including its negative-zero bits. A failed store returns neither result, skips
later operands, and preserves earlier side effects and completed updates.

Array holes, missing/out-of-range/noncanonical numeric reads are Undefined and
fail the Number check without extension or a write attempt. V14 length growth/
shrink keeps sparse storage rules, owned RangeError for invalid uint32 lengths
and fatal results above 1024. Resource exhaustion and authorized exit remain fatal.
Captured bindings, fresh for-iteration cells, callback inference, earlier arguments/
result pairs and saved returns retain their existing GC/finalizer ownership.

Const/TDZ/guard-refined targets, functions/result pairs, Error/result field mutation,
coercive keys and earlier profiles stay rejected. Parenthesized simple targets
work. No new host operation, helper export or ABI/layout is introduced. See the
[contract](architecture.md#number-update-contract).

#### Number bitwise operators (v12 and later)

Narrowed Number operands support `&`, `|`, `^`, `~`, `<<`, `>>` and `>>>`:

```ts
export function main(): void {
  console.log(5 & 3); // 1
  console.log(~5); // -6
  console.log(1 << 31); // -2147483648
  console.log(-8 >> 2); // -2; sign extended
  console.log(-1 >>> 0); // 4294967295; zero filled
  console.log(4294967297 | 0); // 1; wraps to 32 bits
  console.log(1 << 33); // 2; shift count wraps to 0..31
  console.log(NaN | Infinity); // 0
}
```

Inputs follow Number ToInt32/ToUint32 rules: truncate finite fractions toward zero
and reduce integers modulo 2³²; NaN, infinities and either zero become zero. Signed
operations return a Number in the signed 32-bit range. `>>>` returns a Number in
0..4294967295. Shift counts use their unsigned low five bits, including negative,
fractional and large counts. Results are ordinary binary64 Numbers; zero results
are positive zero.

Operands evaluate once, left to right, before conversion. Narrow unions, optional
fields and array reads to Number first, as for ordinary arithmetic. Strings,
Boolean, Null/Undefined, references and opaque results are not coerced. Invalid
static operands report their own source range. BigInt remains unsupported;
bitwise compound assignments are specified below. Older profiles keep their behavior.

Conversion handles the complete binary64 range without overflow traps or system
calls. It adds no allocation, helper export, host operation or ABI/layout version.
Existing operand exceptions, fatal budgets/exit, exact roots, earlier result pairs,
system status and permissions remain active. See the
[contract](architecture.md#number-bitwise-contract).

#### Bitwise compound assignments (v12 and later)

The noncoercive operand rules below describe v12–v28. V29 extends primitive
conversion as described in [primitive compound assignment coercion](#primitive-compound-assignment-coercion-v29), while retaining checked storage.

`&=`, `|=`, `^=`, `<<=`, `>>=` and `>>>=` support mutable initialized variables,
declared static/dynamic record fields and numeric array indices. V14 also admits
array length. Expressions return the computed Number after a successful store:

```ts
export function main(): void {
  let flags = 1;
  console.log(flags |= 4); // 5
  const record = {n: -1};
  const key = "n";
  console.log(record[key] >>>= 0); // 4294967295
  const values = [4];
  console.log(values[0] ^= 1); // 5
  console.log(flags <<= 33); // 10; count uses the low five bits
}
```

Evaluate receiver/key once and snapshot the old value before evaluating RHS.
RHS mutation or rebinding cannot redirect the prepared reference or replace its
old value. After RHS succeeds, require actual Number operands, apply the existing
ToInt32/ToUint32 operation, then perform the checked store. Closed unions may
contain other members; incompatible runtime kinds raise TypeError after RHS
without coercion or the requested write. Statically impossible targets/RHS fail
at their source range. Ordinary bitwise expressions retain their narrowing rules.

Unknown ordinary record names, absent optional fields, explicit Undefined and
sparse/missing/invalid numeric array reads fail after RHS effects. They do not
create a property or attempt a write. An RHS throw keeps its original exception;
undeclared inherited record names fail during the read before RHS. Earlier
effects and successful inner stores remain when an outer operation fails.

Successful writes use current backing/presence/length: v12/v13 retain fatal
current-length bounds, while v14 can recreate/extend valid indices after RHS
mutation. V14 length stores keep RangeError for invalid uint32 results and fatal
results above 1024. Exact selected-field types, original receiver/key/old/RHS roots,
earlier operands/result pairs and saved returns survive GC/calls/finalizers.
Signed/unsigned results remain Number values; no integer value type, new host
operation, helper export or ABI/layout change. Permissions/status and all fatal
budgets remain active. See the
[contract](architecture.md#bitwise-compound-assignment-contract).

#### Object.keys for closed records (v12 and later)

Object.keys returns a fresh dense string array containing present own fields:

```ts
export function main(): void {
  const value: {a?: number; b: number; "2"?: number} = {b: 1};
  value.a = undefined;
  value["2"] = 2;
  const keys = Object.keys(value);
  console.log(JSON.stringify(keys)); // ["2","b","a"]
  for (const key of keys) { console.log(key); }
}
```

Canonical array-index names from 0 through 4294967294 come first in numeric order;
other names retain insertion order. Overwriting a field keeps its position, and
first assigning an optional field appends its non-index name. An absent optional
field is omitted; an explicitly stored undefined is included. Enumeration uses
presence/order metadata, without reading values or converting the object to JSON.
Typed JSON and object-rest records retain their existing order rules.

Evaluate the argument once and return an independent snapshot. Later source
changes do not change the keys array, and mutating that array does not change the
source. All result slots below length are present; removed spare payloads and bits
are zeroed before length publication, so growth cannot resurrect old keys. Key
names use the profile's UTF-16 string representation, including Unicode and NUL.
The existing 32-field/static-data/array/memory/execution bounds remain active.

Inputs may be a checked closed record or bounded array, including its null/undefined
union. A nullish union value throws owned TypeError before enclosing calls proceed.
Primitive boxing, Error/function/result objects, general prototypes/symbols,
spread/generic calls and shadowed Object intrinsics remain unsupported. Earlier
profiles reject this intrinsic. Argument effects require ordinary grants; key
collection preserves system status. No host import, helper export, layout or ABI change.
See the [contract](architecture.md#objectkeys-contract).

#### Object.values for closed records (v12 and later)

Object.values returns a fresh dense array of present own values in the same order
as Object.keys, including explicitly stored undefined and omitting absent fields:

```ts
export function main(): void {
  const source: {a?: number; b: string; "2"?: number} = {b: "x"};
  source.a = undefined;
  source["2"] = 2;
  const values: (number | string | undefined)[] = Object.values(source);
  console.log(JSON.stringify(values)); // [2,"x",null]
  console.log(Object.hasOwn(values, 2)); // 1: explicit undefined, not a hole
}
```

Equal field types retain their type. Compatible field types form an admitted
homogeneous union element type, including existing record/array/Error references.
Optional field types retain Undefined even when currently absent; a nullable source
adds no result member because its absence throws TypeError instead. Empty closed
records produce an empty `undefined[]`. Incompatible unions, such as two different
record shapes or array element types, fail compilation; no any/casts/coercion or
aggregate covariance is introduced.

The argument evaluates once. Values are prepared in rooted typed slots before
allocation/ordering, using existing union widening and exact numeric packing.
Changing a source scalar later cannot alter the snapshot. Nested objects/arrays
remain shared references; changing the returned outer array does not change source
fields. Strings retain UTF-16 units, including lone surrogates. Tail payloads and
presence bits are cleared before publishing the result length, preventing stale
values or child references from reappearing after growth.

The shared key/value enumeration path preserves numeric-key sorting, insertion
order, GC/earlier operands/results, finally cleanup, status and real grants. As
with Object.keys, inputs are closed records or bounded arrays, optionally nullable;
primitive boxing, getters/prototypes/symbols, direct Error/result/function
inputs, generic/spread calls and shadowed Object remain unsupported. Existing
32-field, type, source/frame/root, memory/fuel/deadline bounds apply. No import,
helper export, heap layout or ABI version change. See the
[contract](architecture.md#objectvalues-contract).

#### Object.keys and Object.values for arrays (v12 and later)

Both intrinsics also accept bounded typed arrays. Keys are ascending decimal
index strings; the non-enumerable length is omitted. Values retain the exact array
element type, including existing unions, without adding Undefined for skipped holes.
Both results have independent storage and every slot below their length is present.

```ts
export function main(): void {
  const source: (number | undefined)[] = [, undefined, 2, , 4];
  const keys: string[] = Object.keys(source);
  const values: (number | undefined)[] = Object.values(source);
  console.log(JSON.stringify(keys));   // ["1","2","4"]
  console.log(JSON.stringify(values)); // [null,2,4]
  console.log(Object.hasOwn(values, 0)); // 1: explicit undefined was retained
}
```

The example requires v13+ for sparse literals; v12 admits dense arrays. Deleted
indices and holes created by writable length in v14 are skipped. A nullable
single-array union throws TypeError for null/undefined; mixed record/array inputs
remain unsupported. The source evaluates once before a bounded snapshot walk.
Nested references remain shared, scalar/union payloads retain their exact bits,
and later source mutation cannot change the returned outer snapshot.

Decimal keys are created inside Wasm without host operations. Private enumeration
helpers reserve zeroed backing, retain partial results across allocations and
publish only initialized live slots. Their temporary roots are restored on return;
spare payloads/bits stay zero through later growth. Existing 1024-element and
execution/memory budgets remain active. No host imports, helper exports or ABI/layout
changes. See the [contract](architecture.md#array-own-enumeration-contract).

### `checked-f64-utf16-v13`

#### Sparse arrays (opt-in v13)

Select `"languageProfile":"checked-f64-utf16-v13"` to represent absent indices:

```ts
export function main(): void {
  const values: (number | undefined)[] = [1, , undefined, 3];
  console.log(Object.hasOwn(values, 1)); // false
  console.log(Object.hasOwn(values, 2)); // true
  console.log(JSON.stringify(values)); // [1,null,null,3]
  values[1] = 2;
  console.log(Object.hasOwn(values, 1)); // true
  const mapped = [1, , 3].map(value => value * 2);
  console.log(JSON.stringify(mapped)); // [2,null,6]
  console.log(Object.hasOwn(mapped, 1)); // false
}
```

Reads and at return undefined for absent indices. Map preserves length and holes,
including indices removed before their turn; filter/forEach skip holes, while find
visits them as undefined. Slice copies presence into fresh storage; pop removes the
last index even if absent. Push appends present arguments, and assigning within
length fills a hole. Includes treats a hole as undefined; indexOf skips it.
Object.hasOwn accepts Number indices and String keys, including computed keys;
canonical decimal indices test presence and length is always own. Coercion is
unsupported. Arrays containing only holes need an element type annotation.

JSON writes null for holes and explicit undefined. Checked parsing creates present
entries, so JSON cannot preserve absence; its schema must admit the serialized
nulls. Earlier profiles retain their layouts and v12's missing-map-index trap.
V13 uses host ABI v16/value ABI v12/schema v12 with versioned value/JSON imports;
permissions, recoverable JSON errors and fatal resource limits retain their roles.
The length bound remains 1024. Delete, indexed extension and writable length are
provided by v14 above. Constructors, prototypes and broader array-like behavior
remain pending. See the
[sparse array contract](architecture.md#checked-f64-utf16-v13--sparse-arrays).

### `checked-f64-utf16-v14`

#### Array mutations (opt-in v14)

Select `"languageProfile":"checked-f64-utf16-v14"` for bounded array mutation:

```ts
export function main(): void {
  const values = [1, 2, 3];
  delete values[1]; // length remains 3
  values[5] = 6;   // intervening indices are absent
  console.log(JSON.stringify(values)); // [1,null,3,null,null,6]
  values.length = 1;
  values.length = 3;
  console.log(JSON.stringify(values)); // [1,null,null]
  try { values.length = 1.5; }
  catch (error) {
    if (error instanceof RangeError) { console.log(error.message); }
  }
}
```

Shrinking clears removed values and presence bits; growth creates holes. Delete
returns true for an index, including absence, and keeps length. Deleting the
nonconfigurable length property raises TypeError. Numeric delete keys and literal
keys are supported; dynamic strings/coercion and arbitrary own properties remain
unsupported. Indexed writes accept finite integral indices 0..1023, evaluate the
receiver/key/RHS once in order and check the numeric write domain after RHS effects.
A thrown key/RHS stops the requested mutation. Earlier RHS mutations remain.

Length accepts Number only: negative, fractional, nonfinite or above-uint32 values
raise RangeError. Negative zero sets length to zero. A valid uint32 length above
1024 exceeds the runtime resource bound and remains fatal; execution/ABI limits
also remain fatal. Earlier profiles retain their prior mutation restrictions.
V14 selects host ABI v17 with unchanged value/schema v12 and v6 value/JSON imports.
No new capability or host import is added. See the
[array mutation contract](architecture.md#checked-f64-utf16-v14--array-mutations).

### `checked-f64-utf16-v15`

#### Number exponentiation (opt-in v15)

Select `"languageProfile":"checked-f64-utf16-v15"` for Number `**` and `**=`.
V15 inherits v14's sparse arrays, closures, exceptions and checked JSON, and uses
host ABI v18 with value/schema v12. Rebuild both user binaries together for native
packaging so the companion advertises the new profile.

```ts
export function main(): void {
  console.log(2 ** 3 ** 2); // 512; right associative
  console.log(9 ** 0.5); // 3
  console.log(NaN ** 0); // 1
  console.log(1 ** NaN); // NaN
  console.log(1 / ((-0) ** 3)); // -Infinity
  const record = {n: 2};
  const key = "n";
  console.log(record[key] **= 3); // 8
}
```

Plain `**` requires narrowed Number operands. `**=` admits closed types with a
possible Number pair, then checks actual kinds after RHS success; other kinds
raise TypeError without coercion or the requested store. It prepares receiver/key
and saves the old value before RHS, uses current checked storage after computation,
and returns the computed Number. Optional/sparse absence, inherited lookup timing,
partial chains, GC/finalizers, length errors and fatal limits keep their contracts.
Older profiles reject both operators.

NaN/zero/Infinity rules follow Number exponentiation, including odd-integer signs
for negative zero/infinity, NaN for finite negative bases with non-integral powers,
and NaN for either ±1 raised to an infinite power. Finite ordinary results use
the platform's implementation-approximated binary64 power; exact last bits may
differ across platforms. BigInt and coercive operands are not added.
Unary bases need parentheses, as in `(-2) ** 3`.

The versioned pure `number_power(f64,f64)->f64` import requires no permission and
preserves system status. Each power consumes one existing host-operation allowance;
deadline checks run before and after native math. Budget exhaustion stays fatal
through catch/finally. JSON allocation/retention helpers remain host-free and cannot
call this import. ABI/profile/signature checks reject incompatible artifacts before
guest effects. No value/schema/container version or resource limit is changed.
See the [contract](architecture.md#checked-f64-utf16-v15--number-exponentiation).

#### Basic Math functions (opt-in v15)

Set `languageProfile` to `checked-f64-utf16-v15` in `varta.config.json` for
direct calls to the following Math functions. Arguments must be narrowed Numbers;
results are Numbers. Most compile to guest instructions; `pow` uses the existing
pure Number-power host operation shared with `**`.

| Functions | Admitted arguments |
| --- | --- |
| `abs`, `floor`, `ceil`, `trunc`, `round`, `sqrt`, `sign`, `fround`, `f16round`, `clz32` | Exactly one Number |
| `imul`, `pow` | Exactly two Numbers |
| `min`, `max`, `hypot` | Zero or more Numbers |

```ts
export function main(): void {
  console.log(Math.sqrt(Math.abs(-6.25))); // 2.5
  console.log(Math.floor(-1.2));           // -2
  console.log(Math.ceil(-1.2));            // -1
  console.log(Math.trunc(-1.2));           // -1
  console.log(Math.round(-2.5));           // -2
  console.log(Math.round(0.49999999999999994)); // 0
  console.log(1 / Math.round(-0.5));       // -Infinity (negative zero result)
  console.log(Math.min());                // Infinity
  console.log(Math.max(2, -1, 7));         // 7
  console.log(1 / Math.min(0, -0));        // -Infinity
  console.log(1 / Math.sign(-0));          // -Infinity
  console.log(Math.fround(16777217));      // 16777216
  console.log(Math.clz32(1));              // 31
  console.log(Math.imul(4294967295, 5));    // -5
  console.log(Math.pow(2, 3));             // 8
  console.log(Math.f16round(1.0004882812500002)); // 1.0009765625
  console.log(Math.f16round(Math.fround(1.0004882812500002))); // 1
  console.log(Math.hypot(3, 4, 12));       // 13
  console.log(Math.hypot(NaN, Infinity));  // Infinity
}
```

`round` resolves halfway values toward positive infinity and preserves negative
zero. `sqrt` preserves negative zero and produces NaN for negative nonzero inputs.
NaN and infinities follow each function's Number rules. Arguments run once before
the operation; their errors and permissions retain their existing behavior.
`min/max` evaluate all arguments in order, including arguments following NaN.
`min()` returns Infinity and `max()` returns -Infinity; negative zero wins a
minimum tie and positive zero wins a maximum tie. `sign` preserves NaN and both
zeroes. `fround` rounds through binary32 using ties to even. `clz32` counts leading
zeroes after Number-to-Uint32 conversion; `imul` returns the signed low 32 bits of
the product, without losing bits through a floating-point multiplication.
`pow` shares the Number rules, implementation-approximated finite results and
one-call/deadline budgets of `**`; it needs no permission and preserves host status.
`f16round` rounds directly from binary64 to binary16 using ties to even and returns
the rounded value as an ordinary Number. It preserves NaN, infinities and signed
zero, including underflow to -0. The smallest positive result is 2^-24; values with
magnitude at least 65520 overflow to signed infinity. No binary32 intermediate is
used: it would double-round values such as the example above.
`hypot` computes the square root of the sum of squares with power-of-two scaling
and compensated arithmetic, avoiding the premature overflow/underflow of directly
squaring the original inputs. All arguments execute once before special-value
selection: any infinity wins over NaN, NaN otherwise propagates, and an empty or
all-zero call returns +0. Finite results are implementation-approximated. This
operation uses guest instructions, allocates no argument array and preserves status.

Only direct calls on unshadowed `Math` are supported. Wrong arity, spread
arguments, coercion, unnarrowed unions, computed names, borrowed functions,
other Math properties and methods remain unsupported. Older profiles reject this
subset. It adds no new registered import, permission, helper export or version/limit change
within v15/ABI18. See the [contract](architecture.md#checked-math-intrinsic-subset).

#### Number predicates (opt-in v15)

Direct `Number.isFinite`, `Number.isNaN`, `Number.isInteger` and
`Number.isSafeInteger` calls return booleans without converting their arguments.
Select `checked-f64-utf16-v15` in `languageProfile`.

```ts
export function main(): void {
  console.log(Number.isFinite(2));                   // 1
  console.log(Number.isFinite("2"));                 // 0
  console.log(Number.isNaN(NaN));                    // 1
  console.log(Number.isNaN("NaN"));                  // 0
  console.log(Number.isInteger(-0));                 // 1
  console.log(Number.isInteger(Infinity));           // 0
  console.log(Number.isSafeInteger(9007199254740991)); // 1
  console.log(Number.isSafeInteger(9007199254740992)); // 0
  console.log(Number.isFinite());                    // 0
  const value: number | string = 2;
  console.log(Number.isInteger(value));              // 1
}
```

The first argument may have any supported type, including an unnarrowed union.
Non-Numbers return false. An omitted first argument returns false; additional
arguments execute once in order and are ignored. Errors and fatal exits in those
arguments still propagate. These predicates do not narrow local types.

Only direct unshadowed dot calls are supported; computed names, borrowed methods,
generic calls, spread and other Number methods remain unsupported.
The operations use guest instructions and preserve host status, permissions and
resource limits. Earlier profiles reject them; v15 keeps ABI18/value12/schema12.
See the [contract](architecture.md#number-predicate-intrinsics).

#### Numeric constants (opt-in v15)

The following properties are available as exact binary64 Numbers:

| Namespace | Constants |
| --- | --- |
| `Math` | `E`, `LN10`, `LN2`, `LOG10E`, `LOG2E`, `PI`, `SQRT1_2`, `SQRT2` |
| `Number` | `EPSILON`, `MAX_SAFE_INTEGER`, `MIN_SAFE_INTEGER`, `MAX_VALUE`, `MIN_VALUE`, `NaN`, `NEGATIVE_INFINITY`, `POSITIVE_INFINITY` |

```ts
export function main(): void {
  console.log(Math.PI);                              // 3.141592653589793
  console.log(Math["PI"] === Math.PI);               // 1
  console.log(1 + Number.EPSILON > 1);               // 1
  console.log(JSON.stringify(Number.MIN_VALUE));     // 5e-324
  console.log(Number.isSafeInteger(Number.MAX_SAFE_INTEGER)); // 1
  console.log(Number.isFinite(Number.MAX_VALUE));    // 1
  console.log(Number.isNaN(Number.NaN));             // 1
}
```

`Number.MIN_VALUE` is the smallest positive subnormal, distinct from the smallest
normal number. Dot reads and computed literal String keys are supported, including
parenthesized names/keys. Dynamic namespace keys, namespace aliases and property
descriptors remain unsupported. Writes, updates, deletion and destructuring targets
for these built-in constants produce a compile-time read-only type diagnostic.
Actual local bindings named Math/Number retain their ordinary record/array,
capture and initialization rules. Older profiles retain their existing behavior
and reject unshadowed built-in constant reads.

Constants use guest `f64.const` with no import, allocation or host allowance.
V15 keeps ABI18/value12/schema12 and all current limits. See the
[contract](architecture.md#numeric-constant-properties).

### `checked-f64-utf16-v16`

#### Trigonometry, logarithms and exponentials (opt-in v16)

Select `"languageProfile":"checked-f64-utf16-v16"` for these additional direct
Math calls. V16 inherits v15's guest Math operations, constants, Number predicates,
JSON, closures and sparse-array mutation behavior.

| Functions | Arguments |
| --- | --- |
| `acos`, `acosh`, `asin`, `asinh`, `atan`, `atanh`, `cbrt`, `cos`, `cosh`, `exp`, `expm1`, `log`, `log1p`, `log10`, `log2`, `sin`, `sinh`, `tan`, `tanh` | Exactly one narrowed Number |
| `atan2(y, x)` | Exactly two narrowed Numbers |

```ts
export function main(): void {
  console.log(Math.sin(0));              // 0
  console.log(Math.exp(0));              // 1
  console.log(Math.log(0));              // -Infinity
  console.log(Math.atan2(-0, -1));        // -3.141592653589793
  console.log(1 / Math.asin(-0));         // -Infinity
  console.log(Math.tanh(Infinity));      // 1
  console.log(Number.isNaN(Math.acos(2))); // 1
}
```

Domain errors produce NaN; the specified zero, infinity and boundary cases are
handled explicitly. `atan2` preserves signed-zero and quadrant rules. Ordinary
finite results use the platform's implementation-approximated math and can differ
in their last bits across platforms. No coercion, generic/spread/computed calls or
borrowed methods are added; `Math.random` requires v17.

Each operation uses one pure typed host call, with no permission or result buffer,
and preserves system status. The runtime checks the existing call/deadline budget
before and after native math without a second charge. Budget exhaustion and exit
remain fatal. Host-free JSON allocation/retention helpers cannot reach these calls.
V16 requires ABI19 even if no native Math call is used; older profiles reject the
new methods and older runtimes reject the new ABI. Rebuild `varta` and
`varta-runtime` together for native packaging. Value/schema/container versions and
resource limits remain unchanged. See the
[contract](architecture.md#checked-f64-utf16-v16--native-math).

### `checked-f64-utf16-v17`

#### Random numbers (opt-in v17)

Select `"languageProfile":"checked-f64-utf16-v17"` for direct `Math.random` calls.
V17 inherits v16's Math, Number, JSON, closure and sparse-array contracts.

```ts
export function main(): void {
  const value = Math.random();
  console.log(value >= 0 && value < 1); // 1
  const other = Math.random("ignored", {label: "also ignored"});
  console.log(Number.isFinite(other)); // 1
}
```

Each call obtains one fresh 64-bit OS random word through pinned `getrandom` 0.3.4,
then converts its high 53 bits to an exact nonnegative multiple of 2^-53 below 1.
There is no shared PRNG, saved seed, entropy cache, guest seed control or replay
promise. Distinct executions consume fresh entropy; packaging stores no seed.
The Math API makes no cryptographic guarantee. Missing arguments need no conversion;
extra admitted arguments evaluate once in order and are ignored without coercion.
An argument exception skips later arguments and the random sample.

The typed `number_random: () -> f64` import requires ABI20 and no capability. It
uses one existing host allowance, checks the deadline before/after entropy access,
and preserves status and result buffers. Entropy failure and exhausted budgets
are fatal through catch/finally, with no fallback to timestamps or a fixed seed.
Direct unshadowed dot calls are admitted; spread, generics, borrowing and computed
calls remain unsupported. Host-free JSON helpers cannot reach this operation.
Rebuild both binaries for native packaging: the companion negotiates v17/ABI20
with value/schema12 and unchanged container formats and resource limits. See the
[random contract](architecture.md#checked-f64-utf16-v17--mathrandom).

### `checked-f64-utf16-v18`

#### ECMAScript Number output (opt-in v18)

Select `"languageProfile":"checked-f64-utf16-v18"` for canonical decimal Number
text in `console.log` and `log`, including Number values inside admitted unions.
V18 inherits v17's language, Math, random, JSON and memory contracts.

```ts
export function main(): void {
  console.log(-0);               // 0
  console.log(1e-7);             // 1e-7
  console.log(1e-6);             // 0.000001
  console.log(1e20);             // 100000000000000000000
  console.log(1e21);             // 1e+21
  const value: number | string = Number.MIN_VALUE;
  console.log(value);            // 5e-324
}
```

Finite values use the shortest round-tripping decimal representation, with fixed
notation for magnitudes from 1e-6 inclusive to 1e21 exclusive, and scientific
notation outside that range. Scientific exponents use lowercase `e`, an explicit
sign and no leading zeroes. Both zeroes print `0`; special values print `NaN`,
`Infinity` or `-Infinity`. This reuses pinned ryu-js 1.0.3, already used for JSON.
JSON output and non-Number union values retain their existing contracts, including
Boolean logging as `0`/`1`. Older profiles and their imports keep their old output.

Scalar `log_f64_v2: (f64)->()` and union `value_log_v7: (i64)->()` require ABI21,
with unchanged value/schema12. They consume one existing host allowance per log,
retain output/deadline limits, and preserve status and result buffers. Host-free
JSON helpers cannot call them. Rebuild both binaries for native packaging; launcher
protocol v2 advertises v18/v19/v20/v21/v22/v23/v24/v25/v26/v27/v28/v29 and maximum ABI28. No dependency, permission, value/
container layout or resource limit is added. Primitive `String(...)` is available
in v19; Number method calls, radix formatting and object coercion remain separate work. See the
[Number output contract](architecture.md#checked-f64-utf16-v18--number-output).

### `checked-f64-utf16-v19`

#### Primitive String conversion (opt-in v19)

Select `"languageProfile":"checked-f64-utf16-v19"` for direct `String(...)` calls
on Number, Boolean, String, Null, Undefined and their admitted unions. V19 inherits
v18's language, Math/random, Number output, JSON and memory contracts.

```ts
export function main(): void {
  console.log(String().length);  // 0
  console.log(String(undefined)); // undefined
  console.log(String(true));     // true
  console.log(String(-0));       // 0
  console.log(String(1e21));     // 1e+21
  const value: number | string = Number.MIN_VALUE;
  console.log(String(value));    // 5e-324
}
```

No arguments produce an empty string; explicit Undefined produces `"undefined"`.
Booleans become `"true"`/`"false"`, Null becomes `"null"`, and Numbers use v18's
canonical decimal/scientific text. Strings preserve every UTF-16 code unit,
including NUL and unpaired surrogates. The first value is saved before extra
arguments run. Extra admitted values evaluate once in order and are ignored;
conversion follows all arguments. A throw skips later arguments and conversion.
Local/imported functions called String retain normal binding and TDZ rules.

The `primitive_to_string: (i64)->i32` import requires ABI22 and no permission. It
validates a primitive cell, copies UTF-16 through the existing result buffer, and
returns a string retained in guest-owned memory before buffer reuse. Present
arguments cost one host allowance; the no-argument empty literal adds no import.
Status and pending exceptions are preserved. Deadlines, output/root/heap limits
and the 1 MiB encoded-string ceiling remain unchanged; failures stay fatal.
The helper cannot be reached from host-free JSON allocation/retention exports.

Only direct global calls are added. Object/array/Error/function conversion needs
the remaining object-to-primitive work; mixed reference unions must be narrowed.
`new String`, borrowed/generic/spread calls, Number.prototype.toString/radix formatting require v21. Native packaging requires both rebuilt binaries,
with launcher protocol v2 advertising v19/v20/v21/v22/v23/v24/v25/v26/v27/v28/v29/maximum ABI28 and value/schema12.
No dependency, helper export or container/value layout change. See the
[primitive String contract](architecture.md#checked-f64-utf16-v19--primitive-string-conversion).

### `checked-f64-utf16-v20`

#### Primitive Number conversion (opt-in v20)

Select `"languageProfile":"checked-f64-utf16-v20"` for direct `Number(...)` calls
on Number, Boolean, String, Null, Undefined and their admitted unions. V20 inherits
v19's primitive String conversion and all previous Math, JSON and memory features.

```ts
export function main(): void {
  console.log(Number());           // 0
  console.log(Number(undefined));  // NaN
  console.log(Number(null));       // 0
  console.log(Number(true));       // 1
  console.log(Number(" +1.5e2 ")); // 150
  console.log(Number("0x10"));     // 16
  console.log(Number("0b101"));    // 5
  console.log(Number("0o77"));     // 63
  console.log(Number("invalid"));  // NaN
  console.log(1 / Number("-0"));   // -Infinity
}
```

No arguments produce +0. Present Numbers keep their value and zero sign; Null
becomes +0, Booleans 0/1, and Undefined NaN. Strings trim exactly ECMAScript
WhiteSpace/LineTerminator units, then require the entire numeric grammar. Empty
trimmed strings yield +0. Decimal fractions/exponents, signed Infinity and unsigned
0x/0b/0o prefixes are supported. Decimal and prefixed integers round to binary64
with ties to even; overflow and signed underflow follow Number rules. Wrong digits,
internal spaces, numeric separators, trailing junk, signed prefixed integers and
unpaired surrogates yield NaN. This is whole-string conversion, with no partial parse.

The first primitive is saved before ignored arguments run; extra admitted values
evaluate once in order, then conversion runs. A throw skips remaining arguments
and conversion. Local/imported functions called Number retain normal binding and
TDZ behavior. The pure `primitive_to_number: (i64)->f64` import requires ABI23 and
no capability or result globals. Present conversions use one host allowance and
before/after deadline checks; absent arguments add no import. Existing status,
result buffers and pending exceptions are unchanged, including on NaN results.
The unchanged 1 MiB encoded-string ceiling and all memory/type/frame/fuel limits
still apply. Host-free JSON helpers reject reachable conversions.

Object/reference-containing unions need narrowing until object-to-primitive
semantics are implemented. Wrappers, `new Number`, generic/spread/borrowed calls,
Number.prototype.toString requires v21; wrappers and general object coercion remain separate work. Rebuild both
binaries for native packaging: protocol v2 advertises v20/maximum ABI28 and
value/schema12, with no dependency, helper export or layout/container changes.
See the [primitive Number contract](architecture.md#checked-f64-utf16-v20--primitive-number-conversion).

### `checked-f64-utf16-v21`

#### Number text in bases 2–36 (opt-in v21)

Select `"languageProfile":"checked-f64-utf16-v21"` for direct `value.toString(...)`
on narrowed Number values. V21 inherits v20's primitive Number/String conversions,
Math, JSON, exceptions, closures and memory contracts.

```ts
export function main(): void {
  console.log((255).toString(16));       // ff
  console.log((35).toString(36));        // z
  console.log((1.5).toString(2));        // 1.1
  console.log((255).toString("0x10"));   // ff
  console.log((-0).toString());         // 0
  console.log(Number.MIN_VALUE.toString(2).length); // 1076
  try { NaN.toString(1); }
  catch (error) { console.log(error instanceof RangeError); } // 1
}
```

Missing/Undefined radix defaults to 10. Other admitted primitives are converted
to Number and truncated toward zero; NaN becomes zero. The resulting radix must
be between 2 and 36, otherwise a catchable RangeError is thrown. This check applies
even to zero, NaN and infinities. The receiver and every argument evaluate once
before conversion/validation, including ignored extra arguments. Argument throws
skip later arguments. General reference-containing radix unions need narrowing.

Decimal text uses the canonical v18 formatter. Other bases use lowercase digits,
fixed notation and a shortest significand that rounds back to the original
binary64 value; nearest/even choices use exact rounding intervals. Both zeroes
produce `0`; valid-radix nonfinite values use their usual names. Internal exact
arithmetic uses num-bigint 0.4.8, already locked through the compiler and now a
direct runtime dependency. It does not add a BigInt language value. Numeric text
is bounded to 1077 ASCII bytes; no guest resource limit is increased.

The typed `number_to_string: (f64,f64)->i32` import requires ABI24 and no permission.
It transfers UTF-16 through existing result globals into owned guest strings,
charges one host allowance and checks the deadline before/after formatting.
Primitive radix coercion may add the existing Number-conversion call. Status,
pairs, roots, saved returns and pending exceptions are retained. Budgets stay
fatal; JSON allocation/retention helpers cannot reach the import. Older profiles
reject this Number method; existing Error.toString is preserved. Only direct dot
calls are added: wrappers, prototypes as ordinary objects, borrowing, computed/
spread/generic calls and other Number methods remain separate work. Native
packaging requires both rebuilt binaries, with protocol v2 advertising v21 /
maximum ABI28 and unchanged value/schema12 and container formats. See the
[radix contract](architecture.md#checked-f64-utf16-v21--number-radix-text).

### `checked-f64-utf16-v22`

#### Decimal precision and Number.valueOf (opt-in v22)

Select `"languageProfile":"checked-f64-utf16-v22"` for direct calls on narrowed
Number receivers. It inherits v21 and uses host ABI25 / value ABI12 / schema12.

```ts
export function main(): void {
  console.log((1.25).toFixed(2));          // 1.25
  console.log((1.25).toExponential(2));    // 1.25e+0
  console.log((1.25).toPrecision(2));      // 1.3
  console.log((1.25).toExponential());     // 1.25e+0
  console.log((-0.001).toFixed(2));        // -0.00
  console.log(1 / (-0).valueOf());         // -Infinity
}
```

Fixed/exponential precision accepts 0–100 digits after the point; toPrecision
accepts 1–100 significant digits. Primitive arguments convert to Number and
truncate, with NaN becoming zero. Missing/Undefined means zero for toFixed and
shortest text for toExponential/toPrecision. Exact halfway cases round the
absolute decimal significand upward. Both zeroes format unsigned; negative
nonzero values retain their sign even when rounded to zero. toFixed switches to
canonical text at absolute values >=1e21; toPrecision switches to exponential
notation when the rounded exponent is below -6 or at least the precision.

RangeError ordering differs: toFixed checks precision before handling NaN and
infinities; the other two handle nonfinite receivers after conversion but before
range validation. Every receiver and argument evaluates once, including ignored
extras, before conversion. valueOf returns the original Number, preserving -0,
and evaluates ignored arguments without a host call.

The three formatters use typed `(f64,f64)->i32` imports, no permission, one host
allowance each and owned UTF-16 result snapshots. Primitive precision conversion
can add the existing Number-conversion allowance. Text is bounded to 123 ASCII
bytes using exact bounded arithmetic and the pinned formatter; resource limits,
status and pending exceptions retain their contracts. Native packaging requires
both rebuilt companions advertising v22 / maximum ABI28 / value12. Older profiles
reject these methods. Wrappers, borrowing, computed/spread/generic calls and
locale formatting remain pending. See the
[precision contract](architecture.md#checked-f64-utf16-v22--number-precision).

### `checked-f64-utf16-v23`

#### Guest truth conversion and short circuiting (opt-in v23)

V23 uses guest ToBoolean for conditions, ternaries, loops, unary `!`, logical
assignment tests and existing array predicate callbacks. Ordinary references
are true; primitives and unions follow the Boolean conversion rules below.
Logical `&&`, `||` and `??` return selected operand values, preserving -0, NaN
and reference identity, with the right operand evaluated only when required.

```ts
function choose(value: string | undefined): string {
  return value || "fallback";
}
export function main(): void {
  console.log(choose(""));             // fallback
  console.log(choose("yes"));          // yes
  if ({n: 0}) { console.log("object"); }
  console.log(![1]);                   // 0
  console.log(1 / (-0 && 7));          // -Infinity
  console.log(0 ?? 5);                 // 0
}
```

The selected left branch excludes impossible nullish/reference members, so
`string | undefined || string` produces String. Known literal/type outcomes avoid
unnecessary unions, including selected function values. Other mixed results use
the existing closed union representation; multiple record/array shapes and
function members in dynamic unions remain unsupported. Opaque result pairs need
explicit fields. Static checking still applies to both source operands.

Tests and operand caching use no native truth/nullish call. Selected-value boxing
may allocate within the existing guest limits; roots retain cached operands,
earlier pairs and saved returns across GC and finalizers. Native argument/RHS
effects keep their grants, status and fatal host budgets. Guest fuel/deadlines,
ABI26/value12/schema12 and old-profile behavior remain unchanged. Opt-in v24 adds direct-local truth guards and short-circuit RHS type refinement.
See the [control-flow contract](architecture.md#guest-truth-conversion-and-logical-operators-v23).

#### Boolean conversion and global numeric predicates (opt-in v23)

Select `"languageProfile":"checked-f64-utf16-v23"` for direct `Boolean(...)`,
`isFinite(...)` and `isNaN(...)` calls. This adds guest instructions and reuses
primitive Number conversion; host ABI26 / value ABI12 / schema12 are unchanged.

```ts
export function main(): void {
  console.log(Boolean(""));           // 0
  console.log(Boolean("false"));      // 1
  console.log(Boolean({n: 0}));        // 1
  console.log(isFinite("12"));        // 1
  console.log(Number.isFinite("12")); // 0
  console.log(isNaN("bad"));          // 1
  console.log(isNaN());               // 1
}
```

Boolean returns false for missing/Undefined, Null, false, both numeric zeroes,
NaN and empty strings; other admitted ordinary values are true, including records,
arrays, functions and Error families. It reads exact String lengths and actual
union tags/payloads without allocating or calling the host. Objects do not invoke
numeric/String conversion. Opaque VartaResult pairs require explicit `.value` or
other fields; ignored extras may include complete pairs.

Global isFinite/isNaN convert admitted primitives/unions to Number before testing.
Missing input behaves as NaN. This differs from non-coercing Number predicates.
Narrow Number inputs and missing arguments remain guest-only; other admitted
primitive types use one existing primitive_to_number host allowance. Reference
inputs need narrowing until object ToPrimitive is implemented.

All arguments evaluate once before conversion, including ignored extras. Cache
original inputs across mutation, allocation and throws; preserve roots, status,
pairs and saved returns through finalizers. Global normal bindings/closures/TDZ
retain precedence. Guest fuel/deadlines and fatal host budgets remain enforced;
new permissions, dependencies, helpers and version/limit changes are not added.
Only direct calls are admitted: wrappers, borrowed/computed/generic/spread forms
remain pending. Old profiles retain their admission rules. See the
[conversion contract](architecture.md#boolean-and-global-numeric-predicates-v23).

#### Numeric prefix parsing (opt-in v23)

Select `"languageProfile":"checked-f64-utf16-v23"` for direct global `parseInt` /
`parseFloat` and `Number.parseInt` / `Number.parseFloat` calls. V23 inherits v22
with host ABI26 / value ABI12 / schema12.

```ts
export function main(): void {
  console.log(parseInt("  -0xFtail"));             // -15
  console.log(Number.parseInt("0x10", 10));       // 0
  console.log(parseInt("10", 4294967312));        // 16 (radix wraps to 16)
  console.log(Number.parseFloat("1.25e2tail"));   // 125
  console.log(parseFloat("1e+tail"));             // 1
  console.log(1 / parseInt("-0tail"));            // -Infinity
}
```

Every argument evaluates once before conversion, including ignored extras.
Input accepts admitted primitives/unions and converts to String: Number input
uses canonical decimal text, so parseInt(1e21,10) returns 1. Missing input returns
NaN. parseInt converts its second primitive argument to Number then signed Int32;
zero/missing/Undefined selects base10 with optional hexadecimal prefix, while
explicit base16 also strips that prefix. Other bases2–36 consume their longest
valid digit prefix. Invalid bases/no digits return NaN, preserving system status;
signed zero is retained. Decimal leading zeroes do not select octal.

parseFloat accepts the longest decimal/fraction/exponent or signed Infinity
prefix; an incomplete exponent rolls back to the mantissa. Both parsers skip
exact ECMAScript leading whitespace and stop at nonmatching UTF-16 units,
including NUL/surrogates. Their prefix behavior differs from whole-string Number
conversion. Integer prefixes round nearest/even with bounded exact arithmetic;
decimal prefixes use the checked Rust binary64 parser after grammar selection.

Each present-input parse uses one capability-free host allowance. Checked
primitive cells carry inputs/radix; no result globals or buffer writes are needed.
Integer scratch uses at most 1030 bits and decimal ASCII scratch within half the
1 MiB UTF-16 string ceiling. Roots, pairs, saved returns, status and fatal budgets
retain their contracts. Global lexical functions/closures retain precedence;
Number namespace calls require the unshadowed direct spelling. Wrappers, general
object conversion, borrowing, computed/generic/spread calls remain pending.
Rebuild both native companions for protocol2 v23 / maximum ABI28 / value12. See the
[parsing contract](architecture.md#checked-f64-utf16-v23--numeric-prefix-parsing).

### `checked-f64-utf16-v24`

#### Truth-based local guards (opt-in v24)

Select `"languageProfile":"checked-f64-utf16-v24"` for direct-local truth guards
and RHS type refinement. V24 inherits v23 and shares ABI26/value12/schema12;
rebuild both native companions so protocol2 advertises v24 in its supported list.

```ts
function get(value: {n: number} | undefined): number | undefined {
  return value && value.n;
}
function fallback(value: {n: number} | undefined): number {
  return (value || {n: 8}).n;
}
export function main(): void {
  console.log(get({n: 7}));     // 7
  console.log(get(undefined));  // undefined
  console.log(fallback(undefined)); // 8
}
```

Truthy branches remove Null/Undefined. Falsy branches remove always-true object,
array and Error members but retain Number/String/Boolean: zero, NaN, empty String
and false are still possible. Parentheses/! invert guards normally. A true && path
combines both true guards; a false || path combines both false guards. Other
compound outcomes remain conservative. RHS of && sees a truthy left, RHS of ||
a falsy left, and RHS of ?? a nullish direct-local left. Existing typeof/Array/
Error guards also propagate into these RHS branches.

Guard information is scoped to if/ternary branches, while bodies, for bodies and
updates, or the selected RHS. If/ternary joins retain facts shared by normally
completing branches, including early-exit guards. Loop/label/switch exits use
matching break/continue paths; optional RHS effects remain conservative. Guards do not precede the first do-while body. No property/global/
call-result guard tracking is added.
V24 uses write-aware invalidation and full declared types for mutable captures,
as described above. Guards that remove no members add no capture restriction.
This opt-in boundary preserves v23's existing truth-branch mutation behavior.

Guards add no native operation, capability, dependency or resource limit. Cached
operands, exact roots, status, exceptions, saved returns and source-free packaging
retain their contracts. Unknown profiles, missing language declarations or
mismatched ABI fail before effects. ABI26 intentionally supports both v23 and v24;
metadata is not proof of source provenance. See the
[guard contract](architecture.md#checked-f64-utf16-v24--truth-guards).

#### Write-aware refinements and captures (v24)

V24 permits writes to guard-refined locals while tracking which type facts remain
valid. Stores validate against the declared type and replace the old fact with
the assigned value's type. Physical storage retains the full declared union.
Mutually exclusive branches have their own facts; their join combines types from
paths that complete normally, excluding branches that return, throw or jump.
For example, assigning a record in both branches makes its fields available
after the `if`; `if (!value) return` preserves the remaining truth guard.

```ts
export function main(): void {
  let value: {n: number} | undefined = {n: 7};
  const clear = (): void => { value = undefined; };
  if (value) {
    const saved = value.n;
    clear();
    console.log(saved); // 7: evaluated before the mutation
    if (value) { console.log(value.n); }
    else { console.log("cleared"); }
  }
  const fixed: {n: number} | undefined = {n: 9};
  if (fixed) {
    const read = (): number => fixed.n;
    console.log(read()); // 9: const capture keeps its proven type
  }
}
```

Calls with unknown effects invalidate facts for mutable captured locals. V24
uses known closure summaries and immutable function aliases to invalidate only
matching captured cells. Module calls account for function arguments; calls without
a reachable caller closure preserve local binding facts.
Their closures see the full declared cell type and can perform their own guards.
Proven types of const bindings transfer through nested captures because those
bindings cannot be reassigned. Known non-reentrant native calls keep local facts.
Summaries propagate through functions that capture only other functions; callee-local
writes do not invalidate caller bindings. Mutable/returned function identities and
unknown self/opaque callback calls retain the conservative fallback. Stable
callback parameters are now bound to actual input effects, including forwarding,
const copies and read-only captures. Reassigned inputs or nested stores to those
parameters remain conservative. Default/rest prologues and Function receivers
retain their original argument positions. See the
[callback-input contract](architecture.md#stable-callback-parameter-effects-v24). This does not infer
unchanged record properties or heap aliases. See the
[call-effects contract](architecture.md#proven-call-effects-for-local-cells-v24).

Loop analysis accounts for assignments, updates, patterns, assignment heads and
possible callback mutations before lowering repeated code. Body writes invalidate
update/test facts; `continue` may bypass a later store. Matching continue states
join normal body fallthrough before the update/test, and matching breaks join
false-test exits. Literal-true loops have no false edge; for initializers run once
before repeated-part mutation analysis. Lexical shadows and
parameters are distinct bindings. Tests with writes/callbacks do not create stale
branch guards. Short-circuit/default RHS facts stay conditional; logical assignment
targets receive the selected result type. Switch direct entries/fallthrough, label
breaks and exception prefixes cannot inherit a skipped store. Loop/label/switch
exits now join their actual completion targets. Finalizers transform or cancel
pending exit facts before those joins. Handler/finalizer entry, repeated-body
invalidation and switch selection remain conservative; loop fixed points, more
precise exceptional/call effects and property/alias tracking remain work in progress. Initializers do not introduce
assignment facts. Invalid stale reads require a new guard.

This adds compiler-only `swc_ecma_visit` 29.0.0 for bounded AST effect traversal,
aligned with the existing SWC tree. Runtime distribution has no compiler visitor
dependency. ABI26/value12/schema12, permissions, resource limits and older-profile
rules are unchanged. Roots retain saved arguments, pairs, results and escaping
closure environments across mutation and GC. See the
[write-flow contract](architecture.md#write-aware-refinements-and-captures-v24).

### `checked-f64-utf16-v25`

#### Predicate array searches (opt-in v25)

Select `"languageProfile":"checked-f64-utf16-v25"` for three more methods on
checked typed arrays:

```ts
const values = [1, 2, 3, 2];
console.log(values.findIndex(v => v === 2));     // 1
console.log(values.findLastIndex(v => v === 2)); // 3
console.log(values.findLast(v => v === 2));      // 2
```

Index searches return -1 when no predicate matches; findLast returns Undefined.
Callbacks must accept the element type plus Undefined, since holes and deleted
positions are visited. The callback can also receive index/array and an explicit
thisArg for functions with a declared receiver. Arrow receivers stay lexical.
Arguments execute in source order, including ignored trailing arguments, before
the traversal length is captured. Callbacks can mutate arrays within existing
bounds; matched values remain rooted across mutation and collection.

This is a checked typed-array API; generic array-like objects, borrowed prototype
methods and general dynamic callback coercion remain pending. V24 and earlier
reject these three methods. ABI26/value12/schema12, production limits and the
default i32 profile remain unchanged. Both native companions must advertise v25;
see the [detailed contract](architecture.md#checked-f64-utf16-v25--predicate-array-searches).

### `checked-f64-utf16-v26`

#### Reverse array folds (opt-in v26)

Select `"languageProfile":"checked-f64-utf16-v26"` for Array.reduceRight on
checked arrays:

```ts
const values = [1, 2, 3];
console.log(values.reduceRight((sum, value) => sum - value));     // 0
console.log(values.reduceRight((sum, value) => sum - value, 10)); // 4
const reversed = values.reduceRight((out: number[], value: number): number[] => {
  out.push(value);
  return out;
}, []);
```

Callbacks receive accumulator/value/index/original array. The accumulator keeps
one checked type, inferred from the explicit initial value or the array element
type. Without an initial value, empty and all-hole arrays throw TypeError;
a single present element is returned without invoking the callback. Holes and
deleted positions are skipped; present Undefined values remain ordinary values.
Functions have an Undefined receiver; arrows keep their lexical receiver.

Source, callback, initial value and ignored trailing arguments are evaluated in
source order before the traversal snapshot. Intermediate values remain rooted
across callback mutation and collection. Existing production bounds still apply
to the whole program, including preparation of the input array. V25 and earlier
reject reduceRight. Both native companions must be rebuilt for the exact v26
profile list; host ABI26/value12/schema12 remain unchanged. Generic array-like
objects, prototype methods and fully dynamic accumulator types remain pending.
See the [reverse-fold contract](architecture.md#checked-f64-utf16-v26--reverse-array-folds).

### `checked-f64-utf16-v27`

#### Array literal inference (opt-in v27)

Select `"languageProfile":"checked-f64-utf16-v27"` to infer one admitted element
union from an array initializer:

```ts
const values = [1, "text", undefined];
values.push(2); // Number, String and Undefined are admitted members.
console.log(JSON.stringify(values)); // [1,"text",null,2]
const nested = [[], [1]];             // Empty peer uses the typed Number array.
```

Primitives, existing unions and admitted record/array/Error references can join.
Initializers run once in source order; values and shallow identities remain rooted
through later effects, exceptions and collection. Holes remain absent and do not
add Undefined to the writable element type. Reads keep the usual missing-value
rules. Empty/all-hole nested array peers can use a typed peer in either order.
An outer empty/all-hole array or a peer group without typed array evidence still
needs an annotation. Earlier record/array contextual typing remains available.

Explicit annotations control admitted values; mutable arrays retain exact element
types. Different record shapes or nested array element types, function-valued/
opaque elements and general recursive/dynamic inference remain pending. V26 and
earlier require the previous homogeneous/annotated-literal rules. Rebuild both
companions for the exact 30-profile list; host ABI26/value12/schema12 and default
budgets remain unchanged. Very wide boxed literals can still exceed the 64 KiB
function-body budget. See the [inference contract](architecture.md#checked-f64-utf16-v27--array-literal-inference).

### `checked-f64-utf16-v28`

#### BigInt scalars (opt-in v28)

`checked-f64-utf16-v28` adds exact BigInt literals/annotations, local and function
values, closures, arithmetic/bitwise/shifts, primitive comparison, BigInt(),
String/Number/Boolean conversion and typeof. Native semantic errors are catchable;
capacity, fuel, deadlines and malformed transport remain fatal. Typed methods
include toString(Number/Undefined radix), valueOf, and asIntN/asUintN with explicit
Number width and BigInt value. Arguments and ignored trailing effects run once
before conversion. The existing 1 MiB payload and VM budgets stay in force.

The representation is an immutable managed binary leaf, not a Number or String.
Host ABI27 negotiates pure typed operations and charges native work against guest
fuel; value/schema12 remain frozen. Rebuild both companions for the exact
31-profile negotiation list. BigInt arrays/records/unions/throws, JSON behavior,
compound assignments/updates and complete coercive built-ins remain pending.
The checked corpus promotes scalar addition and adds five positive cases plus a
known unsupported BigInt-array case; it does not claim complete BigInt or JS support.

### `checked-f64-utf16-v29`

#### BigInt graphs and exceptions (opt-in v29)

`checked-f64-utf16-v29` adds BigInt in closed objects, optional fields, sparse
arrays and primitive/reference unions. Arithmetic dispatches primitive union
values once, so ordinary BigInt array reads can participate in arithmetic; mixed
BigInt/Number pairs throw TypeError. Array searches compare BigInt numeric values,
not handles. String/Number/Boolean/BigInt conversion and typeof work on admitted
primitive unions. Existing array callbacks, destructuring, roots and limits persist.

JSON.stringify throws recoverable TypeError for BigInt, including nested values.
Malformed graph metadata, cycles and resource limits remain fatal. Typed JSON
parsing cannot turn a JSON number into BigInt; a Number|BigInt target selects
Number. BigInt can be thrown/caught, retained across finally and copied into an
owned uncaught-exception payload with bounded hexadecimal diagnostics. Aggregate
diagnostic logs display BigInt with an n suffix; JSON serialization stays strict.

V29 selects host ABI28, value/schema13 and exception channel4; tag11 is BigInt and
its managed binary leaf has a real tracing edge from every containing cell/slot.
V28 retains scalar-only ABI27/value12. Rebuild both companions for the exact
32-profile list. Primitive width/radix/update coercion is supported in v29 within
checked storage types; object coercion and the broader CORE ledger remain incomplete.

V29 BigInt compound assignments reuse the checked arithmetic and bitwise operators
for locals, record fields and array elements. Receivers, indices and old values
are evaluated before the RHS, each once, and a failed operation preserves storage.
Admitted unions dispatch on their actual numeric members; mixed numeric operands
throw TypeError. Earlier profiles keep their original compound-assignment rules.

V29 also supports prefix/postfix BigInt increment/decrement, including
Number|BigInt unions. The numeric kind is preserved and the expression returns
its old/new numeric value only after a successful store. Primitive update coercion
also converts String/Boolean/Null/Undefined to Number. Number-capable missing fields
and array holes become NaN; incompatible BigInt-only stores throw TypeError without
mutation. Object ToPrimitive and general inferred storage widening remain pending.

V29 BigInt.asIntN/asUintN accept primitive widths through ToIndex and primitive
values through ToBigInt. String/Boolean values work; Number values throw TypeError
(the BigInt() constructor separately accepts integral Numbers). BigInt.toString
accepts primitive radix values, defaulting undefined to 10 and rejecting BigInt
radix with TypeError. Arguments run once before conversions, and width errors
precede value conversion errors. Object/Symbol coercion remains pending.

For example, in v29 `let value: number | string = "2"; value++` returns Number 2
and stores Number 3. Prefix returns the new numeric value; postfix returns the
converted old value. Source receivers and keys run once, with existing bounds and
root budgets. Older profiles keep their noncoercive update behavior.

#### Primitive compound assignment coercion (v29)

V29 arithmetic/bitwise compound assignments convert cached primitive operands.
For +=, an actual String operand selects concatenation; otherwise Number/BigInt
numeric rules apply. Other compound operators use numeric conversion, including
String/Boolean/Null/Undefined to Number. Mixed Number/BigInt arithmetic throws
TypeError after source effects and before storing. For example, a Number|String
local initially holding "2" can *= "3" to store Number 6, and a String local can
+= 1n to concatenate decimal BigInt text without an n suffix.

Computed results must fit declared mutable storage. Compatible Number-array
holes can become NaN; incompatible actual results throw without changing the
target, while earlier RHS effects remain. Source receivers, keys and old values
are cached once. V12–v28 keep their earlier operand rules. Object ToPrimitive,
comparison/equality semantics and general inferred storage widening remain pending.

#### Ordinary primitive arithmetic and unary conversion (v29)

V29 applies primitive conversion to ordinary arithmetic, exponentiation and
bitwise/shift expressions as well as their compound forms. Actual String
operands select concatenation for +; numeric paths retain exact BigInt and reject
mixed Number/BigInt arithmetic with TypeError. Primitive +, - and ~ convert
Boolean/Null/Undefined/String values to Number. Minus/not preserve BigInt; unary
plus throws TypeError for BigInt while retaining native payload validation.

For example, true + 2 is 3, "6" * "2" is 12, and +"0x10" is 16. Cached operands
run once in source order; ordinary sparse reads do not create properties. V28 and
older profiles keep their previous admission. Object ToPrimitive, full primitive
comparison/equality and broader mutable storage inference remain unfinished.

#### Primitive comparison and equality (v29)

V29 supports ==, !=, ===, !==, <, <=, > and >= across the six admitted primitive
kinds. Abstract equality converts Boolean/String/Number as required, equates null
with undefined and compares BigInt exactly. Strict equality keeps type distinctions.
NaN is unequal and unordered; signed zeros compare equal. Two Strings order by
UTF16 code units. A BigInt compared with a String uses integer-string grammar,
so 1n < "1.5" is false, while 1n < 1.5 is true.

Operands run once in source order; native boundaries validate canonical values
and meter work. Scalar numeric loops retain guest comparisons. The pinned oracle
covers 2048 primitive comparisons. Object conversion, Symbol and unrestricted
value storage remain pending; older profiles keep their previous admission.

### `checked-f64-utf16-v30`

#### Property deletion (opt-in v30)

`checked-f64-utf16-v30` reuses ABI28/value13/schema13 and adds deletion of optional
closed-record fields. Primitive computed keys are accepted for records/arrays.
Receivers and keys run once; deleted fields disappear from hasOwn/keys/values/JSON,
and later writes restore presence and insertion order. Unknown/absent own keys
return true. Required checked fields and array length throw TypeError; dynamic
properties, descriptors, object/Symbol keys and general JS deletion remain pending.
The runtime companion must advertise the new 33-profile list. Older profiles keep
their previous admission. Focused API, artifact, order and GC tests cover this step.

#### Object.entries (opt-in v30)

`Object.entries(record)` and `Object.entries(array)` return fresh dense arrays of
mutable `[key, value]` arrays. Keys follow Object.keys order; absent optional fields
and array holes are omitted, while explicit undefined remains present. Nested
values retain identity. Later changes to source properties leave the snapshot
unchanged, while mutations to shared child objects remain visible.

The inferred pair element type is the admitted union of String and source values;
this checked compiler does not yet infer tuple positions separately. Nullable
single-record/array sources throw TypeError when absent. Missing, null and undefined
arguments also throw; Number/Boolean/BigInt inputs return empty arrays. Ignored
arguments run in source order before enumeration. String inputs enumerate UTF16
code units, including lone surrogates. Primitive unions dispatch by kind. Object coercion, Symbol, mixed aggregate source shapes
and property descriptors remain pending.
The existing 1024-element, memory and instruction budgets apply. Large capacity
and collection tests use an explicitly increased test-only instruction budget.

#### String own-property enumeration (v30)

Object.keys, Object.values and Object.entries accept Strings and unions of the
six admitted primitive kinds. String keys are decimal indices in ascending order;
values are individual UTF16 code units. A supplementary character therefore
contributes two entries, and lone surrogates remain intact. Empty Strings and
Number/Boolean/BigInt wrappers yield empty results; nullish values throw TypeError.
Missing arguments also throw, after evaluating any provided ignored arguments.
V30 keys/values now evaluate ignored arguments before taking their snapshots.

Strings above 1024 code units throw a recoverable RangeError because enumeration
must fit the existing checked array capacity. Fuel/memory limits still apply.
The implementation uses guest string slicing and array helpers, with no per-unit
native calls or new host ABI. Earlier profiles keep their previous call rules.

#### Object.is / SameValue (v30)

`Object.is(a, b)` compares values without coercion. NaN equals NaN, while +0 and
-0 differ. Strings compare UTF16 contents and BigInts compare exact mathematical
values. Different primitive kinds are unequal. Records, arrays, functions and
errors compare by identity, including aliases across compatible error types and
values selected from admitted unions. Distinct objects remain unequal even when
their contents match.

Arguments run once in source order. Missing arguments become undefined, void
calls produce undefined and ignored arguments still execute before comparison.
Numeric comparisons stay inside Wasm; no new ABI, grants or limits are needed.
Native result pairs require explicit fields; Symbol and other unimplemented
value kinds remain outside the checked source model. V29 admission stays frozen.

#### Checked Object.assign (v30)

`Object.assign(target, ...sources)` copies enumerable own properties into checked
record/array storage and returns the original target identity. Arguments run once
in source order before target validation; sources then copy left-to-right. Each
source gets a fresh own-key snapshot after earlier copies, so later aliases see
prior writes. Record insertion/numeric-key order and array holes are preserved.
Explicit undefined is copied; nested values keep their shared identities.

A fresh plain record literal reserves statically known record-source fields in
its original allocation, so `Object.assign({}, source)` can create a copy. These
new fields have optional checked storage; existing literal fields can widen to
an admitted union before allocation. Existing object aliases keep their declared
layouts. Record sources read fields individually, permitting unrelated nested
shapes without an erased all-values union. Array/String sources also copy to
existing declared numeric record fields; Strings enumerate UTF16 units.

Nullish sources are skipped. Number/Boolean/BigInt wrappers and admitted function/
Error objects contribute no enumerable own fields. Nullish/missing targets throw
TypeError after argument evaluation. Array length requires a Number source field;
invalid lengths throw RangeError after retaining earlier copies. Checked field
value guards likewise retain earlier writes on TypeError. Array element types
remain invariant and all capacity/memory/fuel limits remain active.

Primitive target boxing, descriptors/getters/setters, Symbol keys, arbitrary new
properties on existing objects and general shape-changing storage remain pending.
New target fields are inferred only for plain fresh record literals and statically
known record sources; indexed sources require declared storage. Unknown checked
keys retain existing write guards. V29 admission and ABI/layouts are unchanged.

### `checked-f64-utf16-v31`

#### Named interfaces and type aliases (opt-in v31)

Name record shapes, unions, arrays and function types once and reuse them,
locally or across modules. Declarations may appear in any order:

```ts
// types.ts
export interface Tag { name: string; score: number; note?: string }
export type Tags = Tag[];

// main.ts
import type { Tag, Tags } from "./types.js"; // resolves to ./types.ts
type Label = (tag: Tag) => string;

const label: Label = (tag) => tag.name + ":" + String(tag.score);

export function main(): void {
  const tags: Tags = [{ name: "a", score: 1.5 }];
  const first = tags[0];
  if (first !== undefined) console.log(label(first));
}
```

`import type { T }`, `import { type T }`, `export interface`, `export type` and
`export type { T as U }` are supported. A `./x.js` specifier names the `./x.ts`
source; JavaScript files are never read. Named types are exact closed shapes: a
value of `{ name; score }` is not assignable where `{ name; score; note? }` is
expected. Generics, `extends`, recursive types, `Record<K, V>`, re-exports with
`from` and type declarations inside functions are rejected with a diagnostic.

### `checked-f64-utf16-v32`

#### Template literals (opt-in v32)

Template literals build strings from primitive values. Each substitution runs once,
left to right, and is converted like `String(value)`:

```ts
type Options = { userAgent: string };

function activatePath(agentId: number): string {
  return `/api/v1/agents/${String(agentId)}/activate`;
}

export function main(): void {
  const defaults: Options = { userAgent: `varta/${0.5}` };
  console.log(activatePath(42)); // /api/v1/agents/42/activate
  console.log(`${defaults.userAgent} ${undefined} ${10n ** 20n}`);
}
```

Substitutions may be strings, numbers, booleans, `null`, `undefined`, BigInt
values or unions of these. Records, arrays, functions and Error values need an
explicit conversion (for example `JSON.stringify(value)` or `error.message`);
tagged templates and `String.raw` are rejected with a diagnostic.
