# Changelog

## Unreleased

- K8 T2 S5 (second part), selectors: id and attribute selectors with every
  matcher and the `i`/`s` flags, `:has()` with bounded relative selectors,
  and the location, input and tree pseudo-classes (`:any-link`, `:empty`,
  `:read-write`, `:placeholder-shown`, `:default`, `:indeterminate`,
  `:valid`/`:invalid` with HTML constraint validation, `:user-valid`/
  `:user-invalid`, `:open`, `:modal`, `:dir()`, `:lang()`, …) on both hosts;
  the engine session mirrors attributes, field values, text data and
  element states. Pseudo-elements parse and are refused until rendered,
  except `::file-selector-button`. Tailwind acceptance 261 → 275 of 312.
- K8 T2 S4 remainder (first part), borders, corners, overflow and text:
  per-side `border-*-width`/`-style`/`-color` and per-corner radius
  longhands (registry codes 85–100), `<line-width>` keywords and one to four
  `border-width`/`-style`/`-color` values, `border-style: hidden`, the
  flow-relative border and corner properties of Logical 1 as mapped
  properties, `overflow-x`/`overflow-y` (codes 101–102) with `clip` and
  `scroll` and the Overflow 3 axis rule, `visibility` (103), `text-transform`
  (104, case, `full-width` and `full-size-kana` natively), `pointer-events`
  (105), `user-select` (106, with `-webkit-user-select`) and `box-sizing`
  (107). Natively: borders and corners per side, elliptical radii, one-axis
  `clip`, classic gutters for `scroll`, invisible boxes skipped for paint,
  hits, focus and accessibility, hits passing through `pointer-events:
  none`, and transformed text. Shorthands that win over registered
  longhands (`border`, the side shorthands, `border-width`/`-style`/
  `-color`, `border-radius`, `outline`, `overflow`) now give the longhand
  their component in the computed values, so the browser bridge applies
  the cascade's result.
  Tailwind acceptance 251 → 261 of 312 candidates.
- V38 K9 D3 full RegExp (selector `full_regexp`): the whole ECMAScript 2025
  pattern language compiled ahead of time to backtracking programs (format
  2, `varta_common::regexp`) run by the linked `regexp_exec` intrinsic as
  guest code: numbered and named backreferences, lookahead and lookbehind,
  pattern modifiers `(?ims-ims:…)`, duplicate named groups, `\p{…}`/`\P{…}`
  (ICU4X 2.4 data), the `d` flag with match indices and the `v` flag with
  `&&`, `--`, nested classes, `\q{…}` and the Unicode Emoji 17.0
  properties of strings; early errors keep V8's text. Matching work is fuel,
  so a catastrophic pattern ends with the turn's typed fuel exhaustion.
  V37 keeps the linear-time subset unchanged. Conformance: 179 passed, 5
  known unsupported of 184 (four new feature rows and cases).
- V38 K9 type level, step 3 (owner decision): a class array viewed as an
  ancestor array (`B[]` or `(B | C)[]` as `A[]`) is one shared array, as in
  JavaScript; an element read through a narrower name that does not admit
  the element throws `TypeError` (replacing the copy of step 2). Pattern
  template literal types (`` `on${string}` ``, `` `${number}px` ``) and
  `Uppercase<string>` are checked like tsc: literals and template
  expressions must match, `string` is not assignable. Admitted: `infer` in
  intersection patterns, `this` in generic declarations and generic
  declarations referring to themselves, `asserts this` methods and overloads
  with rest parameters. Conformance: 193 passed, 5 known unsupported of 198.
- K8 T2 S5 (first part), selectors: complex selectors with the four
  combinators, type and universal selectors, `:is()`, `:where()`, `:not()`,
  `:root`, `:host`, the structural pseudo-classes with `An+B`, `:focus`,
  `:focus-within` and `:enabled`, Selectors 4 specificity, bounded matching
  charged to the cascade work estimate; a winning `margin`, `padding` or
  `gap` shorthand now overrides its longhands in the engine's computed
  values (the browser bridge applied a losing longhand last). Tailwind
  application candidates accepted: 228 to 251 of 312; the theme's
  `:root, :host` rule is admitted.
- K8 T2 S4 (first part), layout and text properties: CSS Logical 1
  `padding-`/`margin-`/`inset-` `inline`/`block` properties (with
  `-start`/`-end`), `inset` and `flex` map onto physical longhands at their
  place in the cascade (with `var()`, split after substitution); new
  properties `translate`, `scale`, `transform-origin` and `letter-spacing`,
  rendered natively (effective transform `translate`, `scale`, `transform`
  around the origin; shaper letter spacing). Chrome comparison: three new
  scenes pass at 1x, 1.25x and 2x. Tailwind application candidates
  accepted: 203 to 228 of 312.
- K8 T2 font-metric units: `ex`, `cap`, `ch`, `ic`, `rex`, `rcap`, `rch`,
  `ric`, `lh` under `line-height: normal` and `rlh` resolve from the
  element's first available font on both hosts. The style engine selects
  the font (CSS Fonts 4 §5 matching of family, style and weight) and reads
  packaged faces' metrics itself (`read-fonts`, variable faces included);
  the platform measures the fonts it renders with itself (natively through
  CoreText, in the browser through Chrome's own units), and the Values 4
  fallbacks apply only without any font. The engine also computes
  `font-weight` and `font-style` with the user-agent rows (spec `bolder`
  table). The Chrome comparison scenes (three new) pass at 1x, 1.25x and
  2x with no new deviation; the browser host's `monospace` fixed-size flag
  is fixed. New package: `core_maths` (read-fonts' `no_std` floats).
- V38 K9 C owner decisions (2026-11-06), `expression_forms`: tagged
  templates pass GetTemplateObject's per-site template object, so
  `TemplateStringsArray.raw` works and every evaluation of a site sees the
  same arrays; object-literal methods see the receiver they are called on
  (spread copies, methods moved between objects, `undefined` for an
  extracted method), passed through a register by every function-value call
  of programs that have such methods, so spreads, `Object.assign` and
  enumeration of such objects are admitted. Modules evaluate in the order of
  ECMAScript's Evaluate() (requests in source order, type-only imports
  erased), initializers may call imported functions, and module
  initializers may `await`: a synthesized evaluator runs the asynchronous
  module algorithm (siblings run while an awaiting module is suspended,
  dependents run in the job its completion settles, a rejection fails every
  waiting importer) and the entry module calls `main` at its end. Object
  literals have getters and setters (accessor properties): reads, writes,
  compound assignments, views, spreads, own keys, entries and
  `JSON.stringify` (through a materializing copy) go through them; other
  host reads of such objects are refused. No host ABI or value layout
  change (the accepted value ABI bump was not needed); v37 keeps rejecting
  every form.
- V38 K9 D2 symbol values (selector `symbol_values`): `Symbol()` values
  with the additive value18 tag `ValueKind::Symbol` (16), `Symbol.for`/
  `Symbol.keyFor`, the well-known symbols as values, `description`,
  `toString`, `String`/`Number` conversion, `typeof` narrowing, identity,
  and symbols in records, classes, arrays, unions, dictionaries, `unknown`
  and `Map`/`Set` keys; `unique symbol` annotations. JSON skips symbols as
  Node does, diagnostics show `Symbol(description)`, KV refuses them. Value
  ABI number unchanged (18); v37 keeps rejecting every form. Members keyed
  by a module-level `unique symbol` (across modules) or by
  `Symbol.toPrimitive`/`toStringTag`/`hasInstance`, with the ToPrimitive,
  `Object.prototype.toString` tag and `instanceof` protocols.
- V38 K9 type level, step 2 (owner decisions): branded primitives
  (`string & { __brand: … }` is the primitive), `typeof` of local bindings,
  overloaded call/construct signatures and function intersections (calls
  select the overload statically), generic call signatures with inference,
  `this` in interfaces and object type aliases, `keyof`, indexed access and
  mapped/utility types over class instances, arrays, tuples and primitives,
  pattern template types as `string`, `infer` through declared generics,
  union patterns and optional elements, tuple elements after a rest element,
  predicates narrowing to subclasses, from `unknown` to object types and
  `this is T`, and class subtyping in every container (`B[]` as `A[]` is a
  copy). Intentional deviations from Node: a lying predicate throws
  `TypeError` at the narrowed read, and a copied array view does not alias.
  Callable objects with properties stay refused. Conformance: 160 passed, 5
  known unsupported of 165 (new row `ts-class-subtyping`).
- K8 T2 font-relative units: the style engine computes font sizes and line
  heights, so `em`, `lh` and `rlh` work in every property on both hosts;
  registered length values compute to absolute lengths before they inherit;
  sums of lengths and percentages (`calc(50% - 1rem)`) resolve at layout for
  sizes, margins, padding, gaps, flex basis, insets and `vertical-align`
  (Taffy is vendored with a recorded patch for calc resolution).
- V38 K9 type level (selector `type_level`): conditional types with
  distribution, `infer` (constrained, tuple, function, object and template
  patterns) and recursive generic aliases; mapped types with `as` remapping
  and `+/-readonly`/`+/-?` over records, tuples and arrays; template literal
  types and `Uppercase`/`Lowercase`/`Capitalize`/`Uncapitalize`; every
  built-in utility type; `keyof` and precise `T[K]` (also for computed
  record reads); `typeof`/`keyof typeof` queries of module constants,
  functions and classes; record intersections; call and construct
  signatures, `new (…) => T` constructor types and class values for them;
  named, optional, rest and variadic tuple elements and `[]`; type
  predicates and assertion functions (predicate-narrowed reference reads are
  checked at run time); `satisfies`; polymorphic `this`; `never`
  exhaustiveness over literal sets; and tsc's literal inference (`const x =
  "a"` is `"a"`, `let` widens, `as const` readonly literal tuples and
  records), which also lets the Tailwind scanner enumerate literal-typed
  class compositions. Refused by name: `unique symbol`, pattern template
  types outside `extends`, function and branded-primitive intersections,
  overloaded signatures, `typeof` of locals. No host ABI or value layout
  change; v37 keeps rejecting every form. Conformance (with K9 D): 155
  passed, 5 known unsupported of 160 (new origin `typescript_handbook`, nine
  feature rows).
- V38 expressions and functions (K9 C, `expression_forms`): `await` in
  every operand position of an async function (call and `new` arguments,
  operators, templates, conditions, object and array literals, assignments,
  optional chains); the comma operator; `new.target === C` in constructors;
  tagged templates and `String.raw`; spreads of tuples and array literals
  into fixed parameters and `new`; `o.m()` for functions
  with an explicit `this` parameter; class-field arrows typed from their
  parameters and body; object-literal methods that use `this` (copies of
  such objects are refused); constant computed keys; tuple parameter
  patterns (in arrows, functions, methods and callbacks, with tuple rest
  elements), typed array rest and destructuring of strings, collections,
  iterable instances and generators (closed with `return()`); hand-written
  `{ next() }` iterators behind `[Symbol.iterator]()`; `using` and `await
  using` with `[Symbol.dispose]()`/`[Symbol.asyncDispose]()` and `implements
  Disposable`/`AsyncDisposable`; structural thenables in `await` and
  `Promise.resolve` with Node's job order. Generator results are created as
  `{ value, done }`. Still refused: top-level `await`, object-literal
  getters/setters, `TemplateStringsArray.raw`, `SuppressedError`. No host
  ABI or value layout change; v37 keeps rejecting every form. Conformance:
  eleven new feature rows and fifteen cases (with K9 A, B and D: 190
  passed, 5 known unsupported of 195).
- K8 T2 standards items and K8 T3: the browser host runs the Rust CSS engine
  as `view-engine.wasm` (built by `scripts/build-view-engine.py`) and applies
  its computed values, so browser and native hosts accept, refuse and
  compute the same styles; the JavaScript CSS checker, variable resolver and
  custom-property code are removed. The engine adds the CSS-wide keywords on
  every property, `!important` with Cascade 5 layer order, `@layer` inside
  `@media` and `@supports`, `@supports`, named colours, multi-colour
  `color-mix()`, clamped `opacity` and math results, and the desktop shell
  passes macOS reduced-motion and increased-contrast settings as media
  inputs.
- K8 T2 S1–S3, the view CSS engine, by the specifications: stylesheets and
  values are tokenized by CSS Syntax 3 (comments, escapes decoded before any
  check, so no escape hides `url()`, `@import` or `!important`); class names
  follow the HTML class model and escaped class selectors (`.sm\:flex`,
  `.w-1\/2`) match them in `set_class`, TSX and stylesheets; `@layer`
  (Cascade 5 order across sheets), `@property` (initial values, non-inherited
  and syntax-checked registrations) and Media Queries 4/5 (range syntax,
  `not`/`or`, `hover`, `pointer`, `orientation`, `prefers-reduced-motion`,
  `prefers-contrast`, `forced-colors`) as recorded host inputs; math
  functions, every CSS length unit, Color 4/5 colours (`oklch()`, `oklab()`,
  `lab()`, `lch()`, `hwb()`, modern `rgb()`/`hsl()`, `color-mix()`) with CSS
  gamut mapping, `opacity` percentages, numeric font weights, `white-space:
  pre`, `text-align: start | end`, any `transition-property` names and
  `cubic-bezier()`/`steps()`. Values are lowered at computed-value time to
  the grammar the native renderer paints, which fixes Tailwind theme colours
  in `oklch()` through `var()` rendering in Chrome but not natively. The
  browser host's L0 contract is unchanged. Native and Chrome match at 1x,
  1.25x and 2x (`scripts/test-native-css-engine.mjs`); the Tailwind
  application corpus's accepted class uses rise from 25.3% to 70.4%.
- V38 standard library (K9 D, `standard_library`): `sort`/`toSorted` with
  V8's TimSort comparator order, `toReversed`, `with`, `toSpliced`,
  `splice`, `copyWithin`, `concat`, `shift`, `unshift`, `flat`, `flatMap`,
  array `keys`/`values`/`entries`, `Array.of`, mapped and array-like
  `Array.from`; ES2025 iterator helpers and `Iterator.from`; the ES2025 `Set`
  methods; `Map.groupBy`, `Object.groupBy`, `Object.fromEntries`,
  `Object.getOwnPropertyNames` and `hasOwnProperty`; `JSON.stringify`
  property lists and indentation; `AggregateError`; promise combinator
  arrays mixing `Promise.reject`; `Math.max`/`min` spreads; `String`
  `at`/`substr`/`isWellFormed`/`toWellFormed`; and the complete `Date` with
  local time in the UTC zone (a host zone awaits an owner decision). All in
  typed guest code of engine preludes: no host ABI or value layout change.
  `Symbol`, weak references, `Proxy`/`Reflect`, integrity levels,
  `structuredClone` and `Intl` are refused with the reason. Conformance: 146
  passed, 5 known unsupported of 151 (new features `array-copying-methods`,
  `iterator-helpers`, `set-methods`, `grouping`, `aggregate-error`).
- Native `img`: packaged images (a configuration's new `images` map,
  stored in resource section version 2 and carried by the browser bundle
  under the same names) are laid out and drawn by the native renderer to the
  HTML and CSS standards: PNG/APNG, JPEG, GIF, WebP, BMP and ICO by
  signature, converted to sRGB (ICC, `cICP`, `sRGB`, `cHRM`/`gAMA`) with EXIF
  orientation and `image-orientation`, animations on the view clock,
  `srcset`/`sizes`, `width`/`height` hints, `object-fit`, `object-position`,
  `image-rendering`, `aspect-ratio`, replaced sizing, inline images in lines
  of text with every `vertical-align` (a recorded Parley patch for atomic
  inlines), Skia's sampling, rounded content clips, opacity and transforms,
  and named by `alt`; checked against Chrome at four scales. AVIF, SVG, HDR
  images and alternative-text rendering are refused. New dependencies `png`
  0.18.1, `image` 0.25.10 and `moxcms` 0.8.1; the image limits await an
  owner decision.
- V38 module evaluation (K9 B, `module_evaluation`): module initializers may
  call code, as ECMAScript evaluates a module once per instance in source
  order; static fields with calls and `static { … }` blocks run where their
  class is defined, and a read before initialization that the initializer
  can reach is a compile error (JavaScript's `ReferenceError`). Also `super`
  in static methods, re-exports of enums, namespaces and classes with static
  fields, static fields of classes in namespaces and writes of imported
  static fields. Conformance: `static-block` passes.
- V38 class index signatures (K9 B): `[key: string]: T` in a class stores
  undeclared members of its instances in a hidden dictionary, read and
  written by name or computed key; members must fit the index type
  (TS2411).
- V38 enum assignability as tsc 5 decides it (K9 B): any `number` value is
  assignable to a numeric enum type (a numeric literal only if it is a
  member), and string enum types are nominal; `E[k]` with a dynamic key stays
  `string | undefined` (owner decision).
- V38 enum and namespace objects (K9 B): a non-const enum or a namespace used
  as a value is a generated object with JavaScript's keys and order
  (`Object.keys(E)`, `f(NS)`, `typeof E`, `keyof typeof E`). Conformance: two
  new cases.
- V38 declarations and classes (K9 B, `declaration_forms`): numeric,
  string, heterogeneous and `const` enums with constant member expressions,
  enum types as literal unions and the reverse mapping `E[n]`; namespaces
  (nested, dotted, exported across modules) and declaration merging with
  namespaces, functions, classes and enums; abstract classes, methods,
  getters and fields (TS2511/TS2513/TS2515 checks); function, method, static
  and constructor overloads resolved as tsc resolves them, with the
  implementation's result checked against the selected signature; class
  setters and getter/setter pairs, `accessor` fields; static fields as module
  variables with `private`/`readonly` checks and `this` in static methods
  resolved through the called subclass; class expressions bound by `const`
  and classes declared in functions; parameter-property defaults. Static
  blocks, effectful static initializers and decorators are refused by name
  pending a module-initialization design. No host ABI or value layout
  change; v37 keeps rejecting every form. Conformance: eight new feature rows
  (`typescript_handbook` and ECMAScript class references), nine passing
  cases, two known gaps and `class-instance` promoted.
- V38 iteration protocol: a class's `[Symbol.iterator]()` generator method
  makes its instances iterable by `for...of`, array spreads, `Array.from` and
  `yield*`; classes declare synchronous generator methods; array literals
  spread arrays, strings (code points), generators, `Map`/`Set`/`Uint8Array`
  values and iterable instances in source order; `Array.from` copies the same sources;
  synchronous `yield*` delegates to generators, arrays and iterable instances
  and forwards `next`/`throw`/`return` as Node does; `Iterator<T>` and
  `IterableIterator<T>` annotate generator objects and `implements
  Iterable<T>` checks the method. No host ABI or value layout change; v37
  keeps rejecting every form. Conformance: 135 passed, 5 known unsupported of
  140 (new feature `iteration-protocol`).
- K8 T1: Tailwind CSS at compile level. The new tooling crate
  `varta-tailwind` ports the CSS-first compiler of Tailwind CSS v4.3.5 to
  Rust (no Node or JavaScript): candidates, all built-in utilities and
  variants, theme, Preflight, `@apply`, `@utility`, `@custom-variant`,
  `@variant`, `@source inline()` and the theme functions, with output in a
  canonical form (no comments or nesting, static `@supports`, `min-`/`max-`
  media ranges). `"tailwind": {"entry": "src/app.css"}` makes `check`, `dev`
  and `build` generate the stylesheet from the module graph's class tokens
  and serve it as the entry's text resource, deterministically. TSX accepts
  `className` as an alias of `class`. A class name composed from parts whose
  types are finite string-literal unions enumerates every combination
  (bounded); other composed names, and `@plugin`/`@config`, are errors. The
  entry may `@import` project stylesheets by relative path at build time,
  under the module path policy. It matches the official compiler on all
  39,152 oracle candidates; applying full Tailwind output waits for the T2
  stylesheet grammar.
- V38 compiled JSON decoders scan string bodies and whitespace, and copy
  escaped strings, sixteen UTF-16 units per step with fixed-width WebAssembly
  SIMD (`simd128`): 0.75 fuel per scanned string byte instead of 11.5, and a
  276 KB transcript parses in 2.4M fuel instead of 5.9M, with the scalar
  scanner's outcomes, errors and positions. Host contract change: a v38 host
  must support `simd128` (one fuel unit per SIMD instruction); modules of
  earlier profiles are rejected when they contain SIMD, and relaxed SIMD is
  rejected for every module (the reference engine disables it).
- Incremental GC clears large-heap mark bitmaps in bounded work before taking
  its root snapshot, retaining the idle-step fuel bound instead of zeroing all
  bitmap bytes in one step.
- An HTTP/2 connection no longer counts as idle while a response body is
  still streaming (it sent `GOAWAY` and failed the next request), and emoji
  fallback on long text walks its ranges once instead of per cluster.
- Native `varta-shell --system-fonts` on macOS draws colour emoji as Chrome
  does: Apple Color Emoji shaped without its bitmap strikes and drawn by the
  platform face, Chrome's size-dependent emoji advances and line heights,
  whole-pixel placement and emoji presentation selectors. Text and emoji
  under a uniform `scale()` draw through CoreText at the scaled size. Colour
  glyphs on Linux remain refused.
- Native scroll containers show macOS overlay scrollbars as Chrome does under
  the default "Show scroll bars" setting: a scroll (wheel, programmatic,
  scroll-into-view or assistive) shows the thumb, which fades out 500 ms
  later over 250 ms; the pointer on the bar widens the thumb and shows the
  track until it leaves. The bar runs on the view's animation clock, so
  headless renders are deterministic, takes the pointer while shown and
  follows a dark scroller background. `View::set_scrollbar_hover` and
  `FrameMetadata::scrollbar_at` carry the pointer; `varta-shell` presents
  frames only while a bar animates. Compared with Chrome at 1x, 1.25x and 2x.
  Thumb dragging and track clicks remain open.

- Integrated v38 application delivery across the official metered browser host
  and packaged native `varta-shell`, including host effects, OPFS KV storage,
  recorded/replayed network completions and native/Chrome SDK replay checks.
- Native painting now includes horizontal inline edges and solid borders,
  preserved/mixed whitespace, positioned painting, user-agent control defaults,
  number stepping, modal dialogs, native consent and extended macOS accessibility.
- Native system-font mapping, optical size/bold synthesis, color/opacity CSS
  transitions and translated/animated boxes support the console's headless frame
  and button states. Real-window and other-platform acceptance remain open.
- Mixed border color fields can remain on the optional GPU backend. GPU opacity
  groups and typed guest graphics/3D APIs remain separate work.
- HTTP v2 streams Web Response bodies and keeps HTTP/1.1 connections alive under
  separate connection and worker bounds. Web Request ingress, async Wasmtime,
  server TLS/HTTP2 and load/hardening acceptance remain planned server stages.
- V38 application budgets, lifetime promise/timer serials and separate network
  read inactivity handling follow recorded owner decisions; earlier profiles
  retain their declared behavior. See the current development-plan status for
  verification scope and the committed GC follow-up.

- Native text-aware layout shares shaping across adjacent text and nested inline
  elements, preserving whitespace, breaks, semantic font styles, ancestor line
  metrics, cluster ownership and empty anchors. Painting and hits use clipped
  fragments, including overflowing bare text and scrolling. Independent Chrome
  probes cover the bounded subset; later increments add admitted inline edges
  and borders. Mixed block/inline flow and remaining edge cases retain their
  explicit unsupported diagnostics.

- Native lengths resolve `rem` through an explicit host document-root font
  reference across box/grid layout, text, borders and outlines. The default is
  the initial font size; view-root font declarations and media-query units keep
  their separate meanings. Initial application layout and resize preserve the
  supplied metric, with the existing numeric/shaping/paint ceilings retained.

- Native isolated text nodes admit Unicode through the existing bounded shaper,
  including symbols, supplementary characters, combining marks, bidirectional
  runs and Unicode line breaks. General UTF-8, control-character and whitespace
  limits remain in force. Native/Chrome probes compare geometry and glyph order;
  a manifest-driven headless example exports initial-frame diagnostics.

- Native layout stops further text preparation after its first measurement
  failure. Taffy finishes with fallback sizes that are discarded; failed frames
  and layout-command batches publish no partial result, and later requests can
  render normally.

- Native view requests share one charged-shaping allowance across intrinsic/
  repeated measurement and painting, including scaled frames and multiple
  layout commands in a batch. Nested calls preserve accounting; scope cleanup
  permits independent retries. A diagnostic reports the last outer request's
  charged operations without treating them as total CPU or memory usage.

- Native charged shaping operations are reserved before buffer changes,
  recursive lookups and AAT transitions/insertions. Unaffordable batches refuse
  without overspending or forced advancement; exact budgets remain usable.
  Original AAT fixtures cover insertion, kern/kerx and nonadvancing-state refusal.

- Native text shaping uses bounded intermediate arrays and shared per-layout
  output/operation budgets, checks completion before layout publication and
  releases refused or oversized reusable buffers. Maximum ordinary/Unicode
  input remains supported; local pinned HarfRust/Parley patches carry request
  limits, explicit status and capacity statistics.

- Large native combining clusters retain all glyphs, use complete font coverage
  for fallback selection and avoid narrow counter/sentinel overflow. Synthetic
  OpenType fixtures check the unchanged 65536-glyph paint boundary and recovery
  after refusal; coverage mapping no longer needs a fixed-size scratch buffer.

- Maximum-size Unicode field values, password masks, IME replacements and
  placeholders use the existing UTF-16 field allowance throughout native
  shaping and textarea measurement. A local Parley 0.11.1 patch widens
  cluster byte offsets to prevent overflow in long UTF-8 runs. Public text and
  guest/work limits are preserved; whole-text context and source hits survive.

- Native fields reuse their shaped caret/selection layout for glyph painting,
  avoiding a second layout and display-text copy per nonempty field. Plain
  values and placeholders are borrowed; composition and password display
  buffers remain separate from guest values.

- Native preview input now shares one checked keyboard/IME backlog with an
  aggregate owned-text limit. Admission compacts spare platform buffers,
  preserves FIFO and commits, and atomically replaces matching preedit updates.
  Filtering, draining and focus-loss cleanup release accounted text capacity.

- Native view/application APIs now export compact or pretty diagnostic JSON under
  a separate encoded-byte limit. The CLI preserves budget diagnostics and emits
  no partial snapshot on refusal. Export failure leaves the application usable.
  Successful URL-open diagnostics retain a bounded tail of complete URLs and
  report dropped entries without limiting later authorized actions.

- View admission now bounds potential accessible-name evaluation across attached
  and detached components, including repeated relations, native labels and live
  field capacity. Shared role rules avoid charging author-only content reads.
  Native snapshots index names/visibility once and avoid repeated descendant-text
  concatenation at every ancestor.

- View admission now bounds potential expanded component text before style-reading
  commands and final publication. Cached value programs and iterative candidate
  scopes bound custom definitions and standard-property values across attached
  and detached components, without expanding strings. Media, pseudo state and
  winners cannot discount the allowance; pure aliases do not duplicate fresh
  literal text. Standard stylesheet literals retain their full source length.
  Excess rejects the copied transaction before effects, including through the
  Worker, with the prior view retained.

- The official browser renderer now computes custom-property scopes using the
  native resolver's cycle, fallback, inheritance and UTF-16 limit rules. It no
  longer delegates variable-cycle resolution to the browser engine. Private
  literals preserve frozen aliases and font namespaces across media/pseudo-state
  changes, reparenting and stylesheet replacement/removal. Missing variables are
  isolated from ambient page bindings, including shorthand references. Closed
  sessions release media/state listeners and queued refreshes. Root-selector
  rewriting preserves quoted values.

- Native CSS-variable resolution discards zero-length dependency fragments while
  preserving valid empty values. Repeated empty-reference fan-out no longer
  triggers exponential materialization without consuming the output-size limit;
  fallback selection, token boundaries and quoted empty strings keep their meaning.

- Native custom properties now resolve aliases and directed cycles per element
  before inheritance, preserving quoted text and token boundaries. Nested/function/empty
  fallbacks and case-insensitive variable functions are admitted consistently;
  malformed custom values fail early and numeric notes are checked after
  substitution. Resolved values are revalidated by snapshot, geometry and paint.
  A shared dependency arena avoids recursive ownership and exponential cached
  text; per-value limits are checked before expansion. The official browser
  resolver now normalizes the previously observed WebKit outside-cycle difference.

- Shared potential-cascade accounting bounds candidate rule/source work and
  inherited variable discovery across live and detached nodes. Explicit
  style-reading commands and final admission share one allowance; media, local
  edits and equal focus do not discount it. Native cached rule costs and private
  transaction-owned browser indices preserve atomic rejection and reclamation.

- View insertion now enforces a shared component-depth ceiling for attached and
  detached subtrees, including text leaves and reparenting. Excess depth rejects
  the complete transaction at its insertion before external effects. Native
  snapshot/accessibility projection, descendant naming and subtree text collection
  use iterative work lists; the accepted boundary no longer requires a larger
  host stack.

- Stylesheet admission now bounds parsed expansion work per transaction and
  installed inventory before media-condition copies and browser CSSOM parsing.
  Native cascade uses incremental selector indices, borrowed declarations and
  lazy variable maps; display checks avoid resolving unrelated properties.
  JS grammar matching memoizes equivalent paths, admits keyword `var()` values
  and rejects empty declarations consistently with native. Generated browser
  bundles include the checker and typed limit error in DOM/Worker bindings.

- Shared view resource admission now bounds radio reconciliation work and retained
  UTF-8 source payload with sticky live-field reservations. Detached nodes, stale
  writes and deferred parent edits are covered; excess batches roll back before
  effects. The draft field capacity and payload limits are explicit in the
  embedding contract. JS source-size caches are transaction-owned and private.

- Native radio Tab navigation builds one transient document/form/name partition;
  activation reuses captured peer states for unchecking and cancellation. Checked
  stops, backwards movement, hidden/disabled filtering and later membership
  changes retain their behavior.

- UARP mockup tooling now uses the official registry/model/DOM host, with live
  control-state checks and a named-asset loopback server. Browser select replay
  reconciles explicit no-selection, disabled options and duplicate identity
  without setter writes on equal/no-op commits. Pilot browser probes accept an
  optional engine selector for focused development checks.

- Local `varta dev --target browser` serves checked in-memory revisions on
  loopback, with runtime policy/decisions, optional application mode, page budget
  reporting and graceful shutdown. Source/config/resource edits rebuild and
  reload automatically after successful startup; errors preserve the working
  version and appear as host diagnostics. Old sessions stop before reload.
  Browser distribution and dev reject HTTP service contracts explicitly.
  Dev pages now pin all assets under a content-derived revision address;
  mutable root asset aliases are rejected.

- Browser any-HTTPS link consent on every trusted activation: an owned full-URL
  prompt after a checked Worker pause, ephemeral audited decisions, revocation
  recheck and deferred command results. Explicit scope intersections keep their
  direct path; denial, overlap, abort and platform failures do not open an address.

- Browser named event delivery and non-view application sessions: bounded source
  catalogs/generations, owned UTF-8 records, FIFO/coalescing host tasks and explicit
  transport backpressure. Stale deliveries cannot target reopened names; closing
  the last listener can finish a non-view instance. Trusted host gesture turns
  integrate with consent without giving ordinary records gesture authority.

- Browser permission broker inputs, headless query/request answers, bounded
  one-shot sources and audit reports/observers, with native schema/scope/accounting
  checks and additional live browser clipboard restrictions. Parent turn
  acknowledgements bound audit/turn traffic; startup updates and failures are
  checked before guest entry. Explicit origin-local IndexedDB stores atomically
  preserve per-artifact choices; live session revocation is acknowledged by the
  Worker and persists denials without cancelling already authorized transfers.
  Required first-run and optional gesture consent now use a host callback or an
  owned modal outside the guest view; answers resume as separate host tasks,
  without spending turn budgets on human waits. Generated view pages opt into
  the modal/store; imports and bounded executions remain passive/headless.
- Event-source ids stop before f64 rounding; event/network watchers reject
  continuations already owned by timers. Active browser transfers are driven
  before choosing overdue timers.

- Packaged browser fonts from verified TrueType/OpenType/WOFF/WOFF2 bytes, with
  bounded container/decoded-size admission, complete pre-execution sanitization,
  per-session families, stylesheet/variable rewriting and terminal cleanup.
  Original fixtures verify real metrics and simultaneous/ambient-name isolation.
  PNGs and fonts now share aggregate decoded-resource bounds.

- Browser PNG resources: explicit project inventory, pinned no-follow inputs,
  bounded static images, generated file names and SHA-256 identities, complete
  decoding before guest execution, checked Worker/DOM names and owned Blob URL
  cleanup. Other image formats remain in development; fonts are covered above.

- Checked v38 browser view sessions through `mount`, with one commit per turn,
  bounded acknowledged commit/event transport, trusted gesture decisions,
  idle/active turn deadlines, close/remount and bounded `run` rendering. Native
  event accounting and Chrome permission/lifecycle probes pass. Loader imports
  are passive; inert anchors retain navigation prevention after Worker shutdown.
- Preserve empty URL query/fragment delimiters in browser permission matching,
  verified against native exact-scope rules.

- Browser view foundation: structured common-registry metadata, shared native
  child/CSS grammar rules, atomic browser model and private closed-shadow DOM
  adapter. Native batch/CSS/event and real Chrome boundary probes pass.

- V38 browser async network adapter: bounded request/completion slots, exact
  URL/method/visible-header permission ceilings, deferred I/O, per-turn accounting,
  byte/text bodies and cancellation. Fetch omits ambient credentials and never
  follows opaque redirects; CORS/wire-header/encoding limits remain explicit.

- Generated browser timer adapter with bounded stable due queues, cancellation,
  interval/unref behavior, v38 turn budget renewal and exact package/runtime
  `time.timer` permission checks. Real Chrome observations compare native usage;
  permission ceilings remain runtime inputs and optional requests deny headlessly.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/). The
workspace uses package version `0.1.0`; these entries describe the development
snapshot and do not announce a published release. Counts quoted in an entry
describe the state when it was written. Current status is in the
[development plan](docs/development-plan.md#status) and current conformance counts
in [conformance/README.md](conformance/README.md).

### Fixed

- V38 exception/rejection readers apply the profile string limit to Error and
  subclass messages while retaining bounded rejection snapshots; older profile
  limits remain frozen.

- V38 checked record-union conversion treats `unknown` discriminator fields as
  overlapping literal fields. Valid alternatives survive strict matching and
  ambiguous objects raise the existing `TypeError`; older profiles stay frozen.
- V38 checked JSON parsing preserves document insertion order for named record
  properties and numeric order for array-index keys across compiled, linked and
  host builders, including escaped keys, missing optional fields and wide records.
- V38 `if (result.ok)` and related boolean guards narrow discriminated record
  unions, including nested paths; general boolean alternatives remain possible.

- Emit generated conditional spines with an explicit work stack, preserving
  rooting and instruction order while avoiding native-stack exhaustion on large
  checked casts and the UARP console's normal CLI workflow.
- Reject unsupported structural thenables at typed or erased Promise resolution
  boundaries, without calling hooks; resolver failures reject their locked
  promise. Preserve ordinary non-callable `then` properties and nested values.
- Require bounded table maxima in the JavaScript host, closing the `table.grow`
  gap left by initial-size checks.
- Truncate valid fractional timer delays consistently in native and JS hosts.

### Added

- Browser Promise/async microtask checkpoints with bounded token/reaction state,
  native dispatcher/ordering/budget semantics and bounded rejection snapshots.
  Real Chrome fixtures compare logs, rejections and exact resource counters.
  Timers, external event turns and privileged effects remain in development.

- `build --target browser` for the initial checked console bundle, with validation
  before output and no native-root binding or overwrite. Explicit `--target wasm`
  and `--target native` select the existing portable and verified companion paths.
  Broader browser effects and dev integration remain in development.

- Compiler foundation for private browser fuel instrumentation, preserving native
  artifact bytes and matching locked Wasmtime fuel in focused native/transformed
  tests. A Rust-generated frozen ES registry and shared JS tank have matching
  native/Node/Chrome observations on a fixed v38 fixture, including host charge
  boundaries. Official host integration, complete charging and full browser
  acceptance remain in development.
- Initial browser R1 console bundle with SRI/CSP/Trusted Types bootstrap, pinned
  original/image/manifest/Worker integrity, bounded loading and Worker deadline/
  cancellation. Real Chrome cases cover native output, resource limits and file
  substitution. Privileged effects, scheduled execution, broader browsers and
  CLI integration remain in development; host policies are never embedded.
- Trusted browser artifact preparation validates original code/contracts before
  deriving a private image, retaining policies and resolved budgets. Private
  memory/table maxima match native growth ceilings without rewriting portable
  artifacts; preparation does not invoke guests or bind filesystem roots.

- Explicit bounded `wait_network` steps for native headless view scripts. They
  wait for armed network work without advancing virtual timers, cap guest turns
  and maintenance, and terminate on timeout before later inputs. Existing
  lifetime/resource limits and surviving response deadlines are preserved.
  Script validation also covers directly constructed inputs before guest startup.
  Real I/O order remains external; deterministic network replay is still open.

- Verified native server integration: explicit HTTP contract v2 for sync/async v38
  UTF-16/binary64 handlers, bounded wire transcoding, pure completion accessors,
  isolated module initialization and response-owned event-loop work. Capabilities
  are derived from emitted calls and independently validated; legacy v1 semantics
  remain unchanged. Request budgets/ceilings and headless permissions now inherit
  startup policy, listener consent is checked before bind, and v2 failure output
  is delivered by its owning worker. V2 disconnects cancel guest loops and native
  timer/async-network waits; shutdown drains the active response under its original
  deadlines and closes idle connections. Native async DNS now uses a bounded
  process-owned pool, so dropping a request does not wait for an OS lookup. Its
  queue/address bounds also apply to earlier async fetch profiles. V2 workers now
  preserve bounded usage/audit evidence in `RunReport::http` and stream completion
  reports through `RuntimeEvent::HttpRequest` after resource release, including
  failures and disconnects. CLI/companion `--budget-report` streams request usage.
  Synchronous `net.get`/`net.fetch` within v2 requests now use the owned cancellable
  transport and original deadline; cancellation/deadline failures cannot become
  recoverable host results. Status-only fetch and bounded raw get bodies are
  preserved; calls without an owned context retain the previous transport.
  Bounded scripted waits and sampled native resource lifecycle have acceptance;
  broader target/RSS/lifetime and replay work remain open.

- Native server acceptance covers hostile HTTP v2 contracts in portable and
  source-free native artifacts, rejecting metadata/helper/effect substitutions
  before startup or bind. A warmed, isolated cancellation sample verifies closed
  upstream sockets, stable descriptor count and fixed per-request guest memory;
  broader native RSS, target and long-duration acceptance remain open.

- Synchronize the prototype JS contract with distinct Tool/HTTP admission and
  native HTTP v2 registries, with an extractor drift check and shared Node
  differential acceptance. Official browser delivery and fuel remain open.

- V38 directly decodes eligible literal-constrained `JSON.parse(text) as T`
  records, discriminated unions and arrays. A required-tag probe selects one
  record before full validation/build; scalar literal constraints remain checked.
  Unsupported inputs use the original parse/assertion with identical diagnostics,
  single input evaluation and rooted fallback; non-schema target fields retain
  their prior conversion/diagnostic. Public JSON schema, host/value ABI,
  old profiles and literal admission for `varta.json.parse<T>` stay unchanged.

- Reconcile the development plan and CORE/platform ledgers with integrated v38,
  the 2026-11-01 verification checkpoint and remaining acceptance batches;
  distinguish historical probes from current blockers and refresh related overviews.

- V38 compiled JSON decoders (`json_decoders`): every checked parse of a
  static target (`JSON.parse(text) as T`, `varta.json.parse<T>`, untyped
  `JSON.parse` at `unknown`, the preludes' parses) runs a decoder the
  compiler emits for that type, one checker and builder per schema node
  shared by all call sites. It checks the whole input first (grammar, shape,
  duplicates, required fields, limits) with no value allocation, its
  scratch a rooted tape block, then builds top-down in the shared builder's
  layout and order; every failure and the rare inputs it leaves alone
  (escaped keys, index-like dictionary keys, more than 64 dictionary
  entries, nesting deeper than 128, records wider than 64 fields, numbers
  near the binary64 overflow) go to the linked `json_parse_v19`, so values,
  `SyntaxError`/`TypeError` messages and typed limits are unchanged. Long
  number literals convert through the linked `primitive_to_number`. No host
  ABI, value layout or schema changes; v37 and earlier keep their bytes; KV
  decodes stay host imports.
- V38 links checked JSON serialization and the logged text of a value, so a
  v38 module imports effects only (a host implements 51 imports, none of
  them reading value layouts): `json_stringify_owned` is linked (one
  `no_std` value18 encoder, `varta_intrinsics::json`, iterative, with the
  host serializer's output, checks and failure messages: cycles, the depth
  ceiling, the output limit, a callable `toJSON` and the BigInt
  `TypeError`), and `console.log` of a value is `log_string16` of the linked
  `value_format_v12` text (no profile admits `value_log_v12`; under v38
  `log_string16` reads within the module's string limit). Every linked text
  result (Number text, `charAt`, case mapping, `typeof`, BigInt text,
  serialization, logged text) is an owned guest string, so results beyond
  the 1 MiB result buffer succeed within the memory budget and an exhausted
  budget is the typed memory resource; case mapping streams over its input
  in place. Differential tests against the host imports cover the case
  corpus, 288 generated value graphs, the failures and collections. Linked
  serialization costs at most the fair host charge on every measured shape
  since `JSON_STRINGIFY_VALUE` was recalibrated to it (1,500 to 5,500 per
  value at the wave-3b integration; before, 0.56 to 3.31 times the charge).
  The shared value core gains
  `read_record_in`/`read_record_with` over a once-validated `RecordShape`,
  inline word readers and 64-bit slot shifts; the library grows from
  443,815 to 515,798 bytes. Node 24 runs the JSON round-trip fixture with a
  host that implements only `log_string16`, output identical to
  `varta dev`.
- V38 collection between and within turns (`incremental_collection`, wave 3
  (f)). Long-lived application instances run collection work at idle points
  (no runnable task; on the virtual clock of a scripted session, the
  quiescence before the next input) through the host-free guest export
  `varta.collector.idle`, under a separate per-idle-period maintenance
  budget (`maintenanceFuel`, default `budget::MAINTENANCE_FUEL`;
  `maintenanceDeadlineMs`, default `budget::MAINTENANCE_DEADLINE_MS`;
  requested by the manifest, capped by the host ceiling) that is never
  charged to a turn; `--budget-report` lists each idle period that did work
  and a maintenance total. Inside turns the v38 collector is incremental: a
  cycle marks from a root snapshot with a deletion write barrier before every
  reference store (whole-object shading before bulk changes), keeps marks in
  side bitmaps that no value reader sees, sweeps lazily and performs bounded
  work per allocation, while a request the memory budget refuses still
  collects completely first. Collector stress builds verify every store kind
  and the runtime case corpus. V37 and earlier keep their bytes.
- V38 declared response headers and a canonical head (`net.request`): the
  grant takes an optional `responseHeaders` list of exact lowercase names
  (manifest and project config, host ABI 34 only, in a package and a host
  ceiling), and a v38 request shows the guest exactly the declared headers
  the response carries, or the CORS-safelisted names without a
  declaration; a ceiling narrows the set by intersection. `set-cookie` and
  `set-cookie2` can never be declared (a diagnostic says why) and are never
  visible. The head record of `net_request_start_v2` is canonical on every
  host: each lowercase name once in byte order, repeats combined with
  `", "` in arrival order, undeclared fields dropped before the record
  exists, so fair fuel never charges them. `get()` of an undeclared name is
  `null` everywhere. The rule, the regenerated head examples and the
  browser note (`Access-Control-Expose-Headers`) are normative in
  `docs/embedding.md`; v37 heads and digests are unchanged. The prototype
  web host follows the same rule.

- V38 KV storage pays fair host fuel (`kv`): `fuel::KV_OPERATION` (100,000)
  per open, read and commit, `fuel::KV_BYTE` (10) per key or value byte
  staged, committed and returned by a read, `fuel::KV_ENTRY` (2,000) per
  entry returned, and the checked JSON schema and value rates for each
  decode, on top of the call and its descriptor and result bytes. The rates
  are calibrated against guest time (host time per operation, byte and entry
  at the measured guest fuel rate, rounded up); a commit's durable sync and
  opening a file stay bounded by the turn deadline. An unaffordable charge is
  the typed fuel error before the effect, so such a commit is never
  submitted. The `embedding-fair-fuel` block lists the rates.

- V38 callable signatures (`structural_subtyping`): a class method, function
  field, function value or function literal may declare more parameters than
  the type it is assigned to when every extra one is optional, has a default
  or is a rest parameter, so `const a: Api = new Impl()` accepts
  `send(id, options?)` for `send(id)`, as uarp-sdk's trailing
  `options?: RequestOptions` needs. Calls through the narrower type pass
  `undefined` (defaults run in the callee) or an empty rest array. Parameters
  are contravariant (a wider source parameter is adapted, TypeScript's method
  bivariance is rejected with a note), results covariant or discarded by a
  `void` target, and v38 `implements` uses the same rule. Views call methods
  with the full parameter list; a converted function value is a fresh
  adapter closure from one synthesized wrapper per signature pair, so two
  conversions of one function are not `===`. Diagnostics name the parameter
  and both types. Programs without such conversions keep their functions;
  v37 keeps its bytes and diagnostics.

- V38 links the value18 primitive conversions (`primitive_to_string_v6`,
  `primitive_to_number_v6`), the primitive comparisons (`primitive_compare`,
  `primitive_equal`) and the value18 readers (`value_typeof_v11`,
  `value_truthy_v11`, `value_nullish_v11`, `value_is_array_v11`,
  `value_strict_equal_v11`, `value_nullish_equal_v11`,
  `value_is_error_class_v7`): a v38 host implements 45 imports instead of
  56, all effects except checked JSON and `value_log_v12`. Codegen v2 answers
  the common cases inline (safe-integer text two digits at a time, unboxed
  Numbers and strings, digit strings, booleans, `null`, `undefined`,
  same-kind and numeric comparisons, BigInts below 2^32, `typeof` as a
  static spelling) and calls the linked code for the rest; v38 lowering
  narrows the cells of conversions and comparisons to the kinds the
  operands may hold, so code without BigInts or strings links no BigInt
  code or string parser. The reference, vector, dictionary (with the full
  hash-table validation), Error and closure-envelope readers are generic
  over `Memory` in the `no_std` core, and the value8–value17 host readers
  call the same `varta_intrinsics::value`. No measured workload costs more
  fuel (list render 160,137 to 148,445, keyed diff 492,170 to 471,773);
  fractional Number text, non-digit string parsing and comparisons with
  large BigInts cost the linked code's instructions. The keyed view, the
  Markdown fixture, the dictionary workload and the TSX application import
  only `log_string16` or `view_commit`, and Node 24 runs such a module with a
  host that implements only `log_string16`.

- V38 fair host fuel: every host import of a v38 module charges published,
  fixed rates for its work (`varta_common::fuel`, in the embedding contract):
  50 per call, 1 per string argument or result byte, and for checked JSON
  per input unit, value and schema node; a charge the fuel cannot cover is
  the ordinary typed fuel-budget error. A `JSON.parse` of a megabyte is no
  longer free. Earlier profiles charge nothing. The v37/v38 corpus check
  and the codegen v2 ceilings compare like with like (v37 rated at the same
  rates by a test-only switch); the "JSON.parse ~2 KB" ceiling is re-pinned
  at 485,000.

- V38 links checked JSON parsing (`json_parse_v19`): the shared decoder and
  builder run as guest code, with scratch in guest heap blocks rooted for
  the call, the host's `SyntaxError`/`TypeError` and its typed input-limit
  errors. A v38 host implements one import fewer. The 24-member event
  fixture now takes about 1.2M fuel in one turn; the development plan
  recommends a v38 per-turn default of 8M fuel (the owner sets it).

- Checked JSON: implementation unified, output unchanged. The host imports
  `json_parse_v13` to `json_parse_v19` (profiles v10 to v38) run one `no_std`
  decoder over the guest's UTF-16 text (`varta_common::json::scan`), with
  the outcome order of the earlier `serde_json` classification, which the
  differential tests compare it against (a hand corpus and 36,000 seeded
  random inputs and mutations). The descriptor check, a serde-equivalent
  schema reader and the v38 top-down builder are shared `no_std` code too.

- V38 `typeof` of engine globals (`web_bytes`): `typeof NAME` of a global
  the engine provides and the program does not shadow is its constant
  JavaScript `typeof` (`"function"` for classes and functions such as
  `Blob`, `Headers`, `Map`, `fetch`, `setTimeout`; `"object"` for `Math`,
  `JSON`, `console`, `crypto`, `process`, `globalThis`), derived from the
  prelude and intrinsic registries and pinned against Node 24. The operand
  loads no prelude, `typeof Blob !== 'undefined' && body instanceof Blob`
  narrows, and unknown names stay compile errors. Unmodified uarp-sdk
  0.13.1 checks 65 of 65 files per file under v38 (7 before).

- V38 Deno KV storage (`kv()`): `varta.kv.open("/data/app.kv")` opens a Deno
  KV database file under a virtual root with Deno's own SQLite backend
  (`denokv_sqlite` 0.14.0, rusqlite 0.40.1 with bundled SQLite, pinned to Deno
  2.9.4's lock), so Deno and Varta share the file without a migration. New
  exact grants `kv.read` (open, read) and `kv.write` (write; makes an open
  writable), nine host ABI 34 imports (`kv_open`, `kv_read`, `kv_commit`,
  `kv_stage`, `kv_key`, `kv_value`, `kv_entry`, `kv_text`, `kv_close`), a pinned
  no-follow open verified against the descriptor SQLite opened, an exact
  schema-version-3 check before any write, a lock probe, and a strict V8
  serializer codec that re-encodes every Deno-written fixture value
  byte-identically. `Kv.get/getMany/list/set/delete/atomic` with
  `check/set/delete/sum/min/max/commit`, `KvU64` and `KvListIterator` with
  cursors; reads decode strictly into `T` like `varta.json.parse<T>`. Denials
  are recoverable `NotAllowedError`s. Commits fail fast (a busy write lock or
  a spent turn fails before anything is submitted), are reported only once
  durable, and a commit not decided by its cutoff is refused at SQLite's
  commit point through a commit hook, so it is never applied. Synthetic
  fixtures written by Deno 2.9.4 and 2.9.7 are in
  `crates/varta-runtime/src/kv_cases`. `watch` and queues are not available.

- `for await` over a generic class instance first created inside a generic
  method (its instance members were not allocated yet) no longer panics the
  compiler.

- V38 links every BigInt operation: a v38 host implements 54 imports instead
  of 67. `varta-bigint` builds without `std`, and the BigInt operations, the
  value13+ primitive conversions and the abstract comparisons are
  `varta_intrinsics::bigint`, which the v28–v37 host imports now call too.
  Operations with two i32 results are exported packed in one i64 and
  unpacked by a linker-synthesized adapter. The value18 conversions and
  comparisons stay host imports (linked, they cost far more fuel on string
  concatenation and union comparisons), as do JSON and the value18 readers.
  The intrinsic region gains a reserved arena, so small linked calls need no
  memory growth when the guest heap fills the memory budget.

- V38 linked code reports an exhausted memory budget as the typed
  `RuntimeError::Resource` (memory, with the request) through the module's
  `resource_failure`/`resource_request` exports instead of a bare trap,
  reads string arguments in place (`charAt` on 1,024 non-ASCII units: 24,533
  to 738 fuel per call), and reuses freed arena blocks. The region's stack
  and reserved arena are measured (5,016 and 6,984 bytes at most over
  adversarial inputs) and set to 16 KiB each, pinned by a test: one page per
  linking module instead of two to three.

- `varta-common` is the one owner of the value layout: a default `std`
  feature, and without it a `no_std` + `alloc` core (`abi`, `memory`, the
  `value` constants and primitive-cell reader, the `limits` constants,
  `text`) that `varta-intrinsics` and the wasm32 shim depend on with default
  features off. The intrinsics' restated layout constants, cell reader and
  UTF-16 helpers and their agreement tests are gone; linked v38 code reads
  string arguments and cells within the v38 data limit instead of 1 MiB.
  The full verification plan and CI check the core build.

- V38 linked intrinsics run in memory 0: a linked module has exactly one
  memory (no multi-memory, so Safari and embedded engines run it). Emission
  reserves an intrinsic region after the static extension (`varta.intrinsics`,
  `abi::StaticRegion::intrinsic_base`) holding the library's shadow
  stack and the data segments its linked operations reach; the compiler
  relocates those segments through the library's kept wasm-ld relocations,
  checks every stack-pointer write against the region, and starts the
  per-call arena at the guest heap's end. Validation admits one memory for
  every profile and the library's data only inside the declared region.

- V38 hashes dictionaries and mixed records under value ABI and layout 18
  (`dictionary_hashing`): a four-slot storage record (`holes`, `keys`,
  `table`, `values`) with an open-addressing table probed from a seedless
  FNV-1a hash of the UTF-16 key units by allocation-free guest helpers,
  tombstones and vector holes for deletions, compaction before enumeration and
  amortized rebuilds at half load. Key order, output and errors are v37's
  (pinned against Node 24); every reader validates the table against the
  entries and rejects a corrupt one. V38 binds twelve value18 readers (host
  ABI 34), JSON schema 18 and exception channel v9; v37 keeps value17.
  Operations cost 1,200–2,500 fuel at 1k–10k entries where the linear layout
  took 54,000–2,100,000; a dictionary holds at most 2^26 entries.

- V38 code generator hotspots: `push` with one to four arguments appends
  without an argument array, checked element reads load the slot without a
  union cell, string `+` chains concatenate up to eight operands per
  allocation, calls of function values with quiet operands take no roots,
  small literal and grown array backings zero their presence bitmap word by
  word (Wasmtime charges bulk memory per byte) and literals reserve four
  slots, and the allocator keeps blocks up to 280 bytes in exact-size free
  lists. Against the integrated v38 tip: array push + index sum 464,755 →
  112,049 fuel, list render of 100 rows 292,803 → 160,137, the 100-row keyed
  diff 1,785,586 → 1,445,572 (73% of it is now the dictionary's linear key
  search), and a reactive keyed view fits 129 instead of 74 mounted and 51
  instead of 35 updated rows in one production budget. V37 and earlier keep
  their bytes.

- V38 (`strict_members`) rejects a static access to a property the type does
  not declare, as `tsc --strict` does (TS2339): reads, writes, compound,
  update and logical assignments, optional chains, literal computed keys and
  destructuring on records, interfaces, views, class instances and narrowed
  union members fail with `property tool_name is not declared by CallPayload`
  at the property. V3 to v37 keep reading such names as `undefined`.

- V38 takes the per-union member limits from the limits registry
  (`ProgramLimits::union_members` and `json_union_members`, frozen 16 and 7
  for v1 to v37), so a discriminated union of 24 or 200 record interfaces
  compiles; checked conversion looks the discriminant up once and selects the
  member with a literal chain (about 55 fuel per member before the match), and
  an unknown discriminant is a catchable `TypeError`. The limit diagnostic adds
  the counts in every profile: `union has 24 structural members; limit 16`.

- V38 treats a `switch` without `default` over a discriminant path (`u.kind`,
  `e.payload.type`) as exhaustive when its clauses cover every member, and a
  missing clause names the unhandled members in the missing-return
  diagnostic.

- V38 `web_bytes` (wave 2): byte bodies for `fetch` (`ReadableStream<Uint8Array>`
  readers, `arrayBuffer()`, `bytes()`, `blob()`, request bodies from bytes,
  `ArrayBuffer`, `Blob` and `FormData` with a deterministic multipart
  boundary), the `ArrayBuffer`/`Blob`/`FormData` preludes, a typed
  `globalThis` (`crypto`, `process.env`, function globals) with
  `process.env` listing only exactly granted names (a static key reads one
  name through `env.get`), classes merged with a same-named interface whose
  members one `Object.assign(this, …)` provides,
  `Promise.race` over differently typed promises, and strict checked
  conversions to unions of record shapes without a literal discriminant
  (exactly one member must match; ambiguity is a `TypeError`). Three host ABI
  34 imports: `net_request_start_v2` (descriptor version 2),
  `net_body_read_bytes` and `env_snapshot`. The pilot pin moves to uarp-sdk
  0.13.0.
- V38 virtual file roots (`file_roots`): `fs.read`/`fs.print` paths and
  their grants are virtual paths `/<root>/...`. The configuration declares
  roots inside the project (`"fs": {"roots": {"project": "."}}`, pinned
  no-follow when the project loads) and grants on them
  (`"fs": {"read": ["/project/fixtures/**"]}`), so committed configurations no
  longer carry machine paths. Packages name their roots (manifest
  `"fsRoots": ["project"]`, host ABI 34) and the host policy maps each one to
  an absolute directory (`"fsRoots": {"project": "/srv/app"}`) for `run`, the
  companion, packaged applications and `invoke`; an unmapped root, a path
  outside the request or ceiling, traversal, a symlink below the root and any
  absolute host path are denied. V1–v37 keep absolute host paths unchanged.

- V38 text resources: `import css from "./console.css" with { type: "text" }`
  binds the file's verbatim UTF-8 text as a default-imported `string`,
  resolved and loaded like a module (relative, inside the configuration
  directory, pinned no-follow descriptors, graph budgets, read once), stored
  once in the static data and recorded in the module graph. A text passed
  directly to `View.stylesheet` is checked at build time against the L0
  stylesheet grammar, now shared as `varta_common::view::stylesheet`, with
  diagnostics inside the `.css` file. Earlier profiles reject attributed
  imports.

- V38 removes unreachable async functions and methods with their frame
  layouts (v37 roots every suspendable function); generator bodies stay. A
  minimal use of the 1900-model pilot mimic keeps 79 functions instead of
  every resource method. The program-limits measurement is recorded in the
  development plan.

- V38 skips the initialization of module constants that no reachable code
  reads and renumbers the rest, so startup fuel, root slots and static data no
  longer grow with unused constants (100 unread enum objects: 5991 fuel under
  v38 against 45253 under v37; adding 200 more leaves v38 unchanged).

- V38 `varta_resume` dispatches through a static per-layout table and one
  `br_table`: about a dozen bytes per frame layout instead of a ~125-byte
  check block, constant resume fuel, the same checks before any effect and
  unchanged frame admission. Every pre-v38 runtime case runs identically
  under v37 and v38.

- V38 `program_limits`: structural limits come from one registry,
  `LanguageProfile::limits()` (`varta_common::limits`), read by the compiler,
  runtime preflight, catalog, schema and closure readers, artifact and
  launcher readers and the generated tables. V38 admits the WebAssembly
  implementation limits: 1,000,000 functions and types, 7,654,321-byte
  function bodies, a 1 GiB module, and 1,000,000 record shapes, classes,
  unions, array, promise, generator and function types. V1–v37 keep their
  frozen limits, bytes and diagnostics.

- V38 `program_limits` foundation: a multi-page static region under host ABI
  34 with value layout 17 (no value 18). Static data that no longer fits the
  first page goes into an extension after the root stack, declared by the
  `varta.static` section, and the managed heap starts after it; result buffer,
  root stack and exception root keep their addresses. Runtime readers accept
  descriptors, keys and schemas in the module's own extension, data segments
  must lie in the declared region, and v38 instantiation no longer charges
  Wasmtime's per-byte data-segment copy to the guest's fuel. V37 and earlier
  keep the single 64 KiB page, their bytes and diagnostics.

- V38 data limits (`data_limits`, in development): arrays (dense, resizable
  and sparse), dictionaries, `Map`, `Set`, `Uint8Array`, spread arguments,
  strings and JSON are no longer capped at 1024 elements, 1 MiB, depth 64 or
  4096 values; the Wasm-derived limits (2^27 elements, 2^30 string bytes,
  a JSON host depth ceiling of 10000) and in practice the unchanged 8 MiB
  memory budget, fuel and deadline bound them. Sparse backings above 1024
  slots keep value layout 17 with one presence bit per slot. More than 64
  continuation frames may be live (a hashed growable table) and the host
  scheduler tracks up to one pending token per 64-byte frame block of the
  memory budget. `JSON.stringify` binds the new `json_stringify_owned` (host
  ABI 34) and writes results larger than 1 MiB into owned guest strings
  through a host buffer bounded by the memory budget; checked parsing accepts
  large and deep inputs and builds them iteratively. Exhausted resources are
  reported as `RuntimeError::Resource` (`resource limit: memory budget
  exhausted (requested …)`) through the new `resource_failure` and
  `resource_request` exports instead of an anonymous trap. The v38 heap grows
  geometrically and marks through a worklist (1000 concurrent awaits: 83.7M
  to 7.7M fuel), and `Array.prototype.join` is O(n log n). V37 and earlier
  keep their bytes, limits and diagnostics. The data limits are the `data`
  field of the program-limits registry, and runtime readers take both through
  one `value::ModuleBounds` parameter. Dictionary hashing is pending a
  value18 storage slot.

- V38 turn budgets (selector `turn_budgets`, increment P5). A v38 instance
  runs as event turns: `main` with its microtasks, then one timer, network
  completion, host event or permission answer per turn. Fuel, host calls and
  microtasks renew per turn, the deadline too in a long-lived application
  instance; memory and output stay per instance, and an exhausted budget names
  its turn. Manifests with `abiVersion` 34 may request `budgets`
  (`fuelPerTurn`, `deadlinePerTurnMs`, `memoryBytes`, `hostCallsPerTurn`,
  `outputBytes`), capped by the host ceiling and by the production defaults
  without one. The runtime gains long-lived application instances
  (`start_application`, `deliver`, `step`; a trap terminates the instance and
  nothing restarts it), host event sources (`varta.events.listen`/`close`, FIFO
  host tasks with coalescing) and permission broker v1:
  `varta.permissions.query` and user-gesture-only `request`, grant
  `required`/`purpose`, decisions keyed by the artifact's SHA-256 identity, a
  `PermissionPrompter` seam and a bounded audit log; headless hosts never
  prompt. Seven host imports use host ABI 34. `varta dev`, `varta run`, the
  runtime companion and packaged applications print output captured before a
  failure ahead of its diagnostic, and accept `--budget-report`, `--app` and
  `--decisions PATH`; `inspect` reports the application identity. Earlier
  profiles keep their budgets, texts and event loop.

- V38 code generation v2 (`codegen_v2`) and the fuel profiler. Quiet
  expressions skip root marks and operand roots, quiet statements skip their
  temporary reset, direct calls with quiet arguments and frames without
  references lose their bookkeeping, operand caches of plain reads collapse,
  statement-level `x++`/`x op= e` become plain assignments, string equality
  uses a chunked content-equality helper (inline against literals of up to 16
  bytes), local `s += t` appends in place into amortized builder blocks,
  `charAt`/`charCodeAt` and safe-integer `%` run in the guest without host
  calls, and the allocator bounds its first free-list scan and grows below a
  collection threshold. Measured workloads use 1.2× to 15× less fuel (a
  `x = x + i` loop 6.8×, 3-char string `===` 3.5×, `s = s + "x"` 15×, a
  100-row list render 5.8×); v37 and earlier keep their bytes and every case
  program behaves identically under v38. `varta dev --profile-fuel [--json]`
  runs an instrumented profile build with exact probe refunds and prints self
  fuel, total fuel and calls of every function, prelude member and compiler
  helper to stderr; profile builds are never packaged. The profiled run is
  the ordinary run path (`RunOptions::fuel_profile`) with the same manifest
  budgets, decisions and execution flags, spans every v38 turn and coexists
  with `--budget-report`.

- V38 structural subtyping (`structural_subtyping`, in development): a record
  or class instance with more members is assignable to an interface or
  object type it provides, without a copy (identity and writes are shared;
  each access through such a view is one accessor call and one descriptor
  test per source layout, and types that never receive a wider value keep
  direct slots). Mismatch diagnostics name both types and list missing,
  mismatched and extra members. The same selector admits generic method
  signatures in interfaces (erased, with checked results), `object`
  annotations, `Array.isArray` narrowing of `unknown` to `unknown[]`, object
  spread of records and optional parameters, `obj.m?.()` on class methods,
  property narrowings kept across calls and getters (reads checked),
  `JSON.stringify` typed `string`, literal `??` fallbacks, `record as
  Record<string, T>`, records held by `unknown` read as JSON objects,
  `Promise<unknown>` arrow contexts, `as void`, `for...of` over strings,
  exhaustive literal `switch` statements, constructor parameter properties,
  and `encodeURIComponent`,
  `encodeURI` and `URIError` in a new prelude. No host import, value tag or
  layout changes; v37 keeps its bytes.

- V38 links pure intrinsics into the guest module (`linked_intrinsics`, first
  increment): Math and `**`/`%`, Number text and parsing, primitive-cell
  conversions and UTF-16 character access and case mapping are compiled from
  the new `no_std` `varta-intrinsics` crate, which the native runtime also uses
  for the host imports of earlier profiles. The compiler links only the
  reachable library functions and data from a checked-in wasm32 artifact
  (`scripts/build-intrinsics.py`) onto a private, bounded second memory; a v38
  module that imports one of these operations is rejected, so a v38 host
  implements 59 instead of 93 admissible imports. V38 Math is bit-identical on
  every host (portable `libm`); JSON, the value17 readers and conversions,
  primitive comparisons and BigInt remain host imports. With code generation
  v2 each operation keeps one v38 path: `charAt` and `s[i]` serve ASCII units
  from static data and other units through the linked `string_char_at16`,
  `%` falls back from its integral fast path to the linked remainder, and an
  integral `toString(radix)` takes an exact integer-digit path. Linked code
  counts against v38's Wasm structural limits, and its private memory's image
  is covered by the v38 instantiation allowance.

- V38 view protocol and TSX (`view_protocol`, `tsx`): the `view_commit` host
  import (host ABI 34) validates each L0 draft-v0 batch as a whole against the
  `varta_common::view` registry and applies it atomically (an invalid batch or
  a second commit in a turn ends the instance); event records reach the guest
  through the compiler-synthesized `varta.view_abi.event` export and the
  engine view prelude (`View.*`, `ViewEvent`). The new `varta-view` crate is
  the headless renderer: node tree, stylesheet cascade subset, bubbling, key
  filters, edit counters, gestures, a deterministic tree and accessibility
  snapshot (no layout yet). `ui.open_url` (with the any-HTTPS form
  `https://*`, valid for that id only) and `clipboard.write` are registered
  with exact grants; every outcome is a recoverable `command_result`. `.tsx`
  modules compile JSX ahead of time into static L0 operations with getter
  holes bound through an installable effect runner; `varta.d.ts` carries a
  generated `JSX` namespace. Application instances host the view: every
  record is a host task that runs as its own turn with fresh budgets (the
  task's gesture flag gates `copy_text`, coalescible records follow the task
  queue's rule), platform inputs are host work between turns
  (`Application::view_input`), and the view capabilities are decided by
  permission broker v1 (audit log, revocation; an any-HTTPS link prompts with
  the full URL on every activation through the prompt seam and is never
  stored). `varta dev --view-script` runs a scripted headless application
  session with the `dev` execution flags and prints the snapshot.

- Opt-in v38 (batch: program and data limits, turn budgets, linked
  intrinsics, view protocol and TSX, structural subtyping and code generation
  v2, in development) is registered. It inherits every v37 feature, shares
  value ABI/layout 17 with v37 and declares its own host ABI 34
  (`PROGRAM_LIMITS_VERSION`) without a new import yet; the launcher advertises
  host ABI 34. A v38 module relabelled as v37 fails at its host ABI section,
  and runtimes that do not list v38 reject its declared profile name before
  any guest effect. Its `program_limits`, `data_limits`, `turn_budgets`,
  `linked_intrinsics`, `view_protocol`, `tsx`, `structural_subtyping` and
  `codegen_v2` selectors admit nothing yet.

- The v37 streams integrated, with their cross-stream rules. Unreachable
  functions, including unused engine prelude members, are removed before the
  256-function limit applies (a `fetch` hello world went from 228 linked to
  154 reachable functions). `JSON.stringify` imports the new
  `json_stringify_v17` (host ABI 33), which skips private state: `#private`
  fields, engine prelude internals (`Headers`, `AbortSignal`, `Request`, … as
  `{}`) and non-enumerable Error slots, so `JSON.stringify` of `unknown` works
  in programs that load Web types; object keys starting with `#` are reserved,
  and `URL` and RegExp match arrays stay out of `JSON.stringify` and
  `unknown`. `Date.now()`/`new Date()` without `time.now` throw a catchable
  `NotAllowedError` through the permission broker instead of ending the run.
  Index reads produce `undefined` where code expects it (`T | undefined`
  contexts, `??`, bindings and loop variables compared with `undefined`,
  `unknown[]`). String `split` has one dispatch path for string and RegExp
  separators. `fetch` and `Request` accept `URL` inputs; classes may
  `implements AsyncIterable<T>`; `instanceof` and `#x in` test every class
  member of a union; readonly literal fields keep literal types; fields take
  their type from `new Map<K, V>()`, `new Set<T>()` and `new Uint8Array(n)`;
  a local named like a prelude value (`fetch`) shadows it. The conformance
  inventory is pinned to v37. Debug and test builds optimize the Cranelift
  crates, removing a `build --single` probe timeout under parallel test
  load. V36 and earlier keep their bytes and diagnostics.

- The language core under opt-in v37 (selector `language_core`). Closures may
  capture `let`/`const` bindings and hoisted nested function declarations
  before their initialization, with JavaScript's temporal dead zone: an
  access through the closure before it throws an Error whose `name` is
  `ReferenceError`. `let x: T;` is admitted with TypeScript-style definite
  assignment. `for (const x of xs)`, `xs[i]` and `s[i]` have the element type
  through a checked read (a hole or a missing index throws `TypeError`, where
  Node reads `undefined`). Guards on property paths (`a.b !== undefined`,
  `typeof a.b.c`, truthiness, `a.b?.c`, literal comparisons) narrow later
  reads until a call or property write may change them. `switch` selects by
  strict equality over strings, literal unions and `typeof`, with clause
  narrowing. Value-returning functions convert to `void`-result function
  types, and function mismatches name both signatures. Endless loops end
  value-returning functions, uninitialized `T | undefined` class fields start
  as `undefined`, `new C()` types a field, constructor arrows may use
  `this.field` and `ReturnType<typeof setTimeout>` is the timer handle type.
  Unions may hold several record shapes, interfaces and classes and arrays
  of different element kinds, told apart by layout descriptor and element
  kind: discriminant, `in`, `instanceof`, `typeof` and `Array.isArray`
  narrowing, field reads and method calls on the union, `never`
  exhaustiveness, and checked JSON conversion that selects the member by its
  literal discriminant. Module-level `let`/`var` hold mutable module state in
  root slots (exports are live bindings), module constants may hold
  functions, and `const id = setInterval(() => clearInterval(id), ms)`
  compiles.
  No value tag, layout, host import or budget changes; v36 keeps its bytes
  and diagnostics.

- Class generics under opt-in v37 (selector `class_generics`): generic
  instance, static and async methods (monomorphized per type-argument list,
  inferred from arguments and from the contextual result type, so the pilot's
  `return this._client.request(spec)` instantiates `request<R>` from the
  method's `Promise<R>`), overrides of generic methods dispatched per
  instantiation, generic classes (`class Page<T>`, `extends Page<T>`,
  `class Numbers extends Page<number>`, statics on the template), method
  signatures in interfaces as function fields with object-literal methods
  that do not use `this`, checked `implements`, and `#private` fields,
  methods and getters with `#x in value`. No value tag, layout, host import
  or budget changes; v36 keeps its diagnostics and bytes.

- Collections and binary text under opt-in v37 (selector `collections`):
  `Map`/`Set` (insertion order, SameValueZero and identity keys, a hashed
  index, live iteration, `forEach`, `keys`/`values`/`entries`, spreads and
  `Array.from`, module-constant sets), tuples (`[A, B]`, `readonly`, trailing
  optional elements; `Object.entries` returns `[string, T][]`), `padStart`,
  `padEnd`, `toUpperCase`, `toLowerCase`, `split` with a string separator,
  `concat`, `codePointAt`, `String.fromCharCode`/`fromCodePoint` and
  `Array.prototype.join`, `Uint8Array` (ToUint8 writes, `set`/`slice`/`fill`/
  `indexOf`/`includes`/`join`), UTF-8 `TextEncoder`/`TextDecoder` with
  `fatal`, `ignoreBOM` and streaming, `btoa`/`atob`,
  `crypto.getRandomValues`/`randomUUID` (the `Math.random` entropy path) and
  a UTC `Date` subset (ISO-8601 and IMF-fixdate parsing, `toISOString`,
  `getUTC*`). They are typed guest code in engine builtin preludes. Three host
  imports with minimum host ABI 33: `string_to_upper_case16` and
  `string_to_lower_case16` (pure) and `date_now` (exact `time.now` grant,
  recoverable `NotAllowedError` denial). No value tag or layout: v37 keeps value layout 17, and
  logging, `JSON.stringify` and `unknown` reject values holding these
  objects. V36 keeps its bytes and diagnostics.

- RegExp under opt-in v37 (selector `regexp`). Regular expression literals
  and `new RegExp` with string literal arguments are compiled ahead of time
  (ECMAScript 2025 pattern syntax, Annex B without `u`, flags `gimsuy`) into
  matcher programs that one guest Pike VM runs in time linear in the input,
  metered as ordinary fuel, so no pattern can backtrack catastrophically. The
  engine prelude provides `RegExp` (`exec`, `test`, `lastIndex`, `source`,
  `flags` and the flag getters), `RegExpExecArray` and `RegExpMatchArray`,
  and the string methods `search`, `match`, `matchAll`, `replace`,
  `replaceAll` and `split`, with replacement templates, replacer functions
  and string patterns. Case-insensitive matching is resolved at compile time.
  Backreferences, lookaround, `\p{…}`, modifiers, run-time patterns and the
  `d`/`v` flags are rejected by name. No value tag, layout, host import,
  capability or budget changes; v36 and earlier keep their diagnostics.

- Network requests under opt-in v37 (selector `net_request`). `fetch`,
  `Request`, `Response`, a `ReadableStream` subset, `URL` and
  `URLSearchParams` are engine preludes over one new exact capability,
  `net.request` (URL scopes over `http`/`https` with an optional method list;
  host ceilings narrow both). Requests run on hyper's HTTP/1.1 client, HTTPS
  through rustls with the compiled-in Mozilla roots; completions wake a
  scheduled frame like a timer, so the runtime keeps only ids and serials.
  Bodies stream as bounded, UTF-8 decoded text chunks (bytes wait for
  `Uint8Array`); `AbortSignal` and `AbortSignal.timeout` cancel transfers;
  redirects default to `error`, with `manual` and grant-checked `follow`.
  Classes may declare `async *[Symbol.asyncIterator]()` for `for await`. Six
  host ABI 33 imports join the registry (`net_request_start` with
  `net.request`, and five bookkeeping operations).
- A permission broker for asynchronous capabilities under v37: a denied
  `net.request` or `time.timer` is decided before any effect, logged, and
  delivered as a catchable `NotAllowedError` `DOMException` (`fetch` rejects,
  `setTimeout` throws through the new `timer_start_v2`); v36 keeps its fatal
  timer denial. The broker is the documented seam for a later consent prompt.
- New dependencies, exactly pinned: `rustls` (ring provider), `tokio-rustls`
  and `webpki-roots` for HTTPS, hyper's `client` feature (adds `want` and
  `try-lock`), and the test-only `rcgen` for loopback certificates.

- Opt-in v37 (pilot batch: language core, class generics, collections and
  binary text, RegExp and `net.request`, in development) is registered. It
  inherits every v36 feature, shares value ABI/layout 17 with v36 and declares
  its own host ABI 33 (`NET_REQUEST_VERSION`), under which the batch's
  imports were added later; the launcher advertises host ABI 33. A v37 module relabelled as v36 fails at its
  host ABI section, and runtimes that do not list v37 reject its declared
  profile name before any guest effect. Its `language_core`, `class_generics`,
  `collections`, `regexp` and `net_request` selectors admitted nothing at
  registration.

- Checked declared fields under opt-in v36 (selector
  `checked_declared_fields`). A mixed record indexed by `unknown` may declare
  fields of any type a JSON value can describe, such as the pilot's
  `errors?: Array<{ field?: string; message?: string }>`, nested records,
  dictionaries, unions and nested mixed records. A declared read returns the
  stored value when its static type is proven (descriptors, element kinds, and
  non-empty exact arrays) and otherwise converts the JSON graph into a fresh
  value that it stores back; checked parsing and assertions from `unknown`
  convert the same way, and a plain `Record<string, T>` asserts to a mixed
  record of the same storage. Structural record assertions accept every key
  order of a shape. V36 also destructures dictionaries, narrows
  `typeof a.b === "string" ? a.b : x`, types `a ?? []` by `a`, admits
  `let x: unknown;` and exhaustive `switch` returns. No value tag, layout,
  host import or budget changes; v35 keeps its diagnostics.

- Timers under opt-in v36 (selector `timers`, K4.4 step 5): `setTimeout`,
  `setInterval`, `clearTimeout`, `clearInterval`, the
  `new Promise<void>((resolve) => setTimeout(resolve, ms))` sleep pattern and
  `AbortSignal.timeout` run on a runtime event loop after the microtask
  checkpoint, in Node's order (main and its microtasks, then timers by due
  time and creation order, each followed by its microtasks). Ids are numbers,
  as in browsers. Arming a timer requires the new exact boolean grant
  `time.timer` (`"time": { "timer": true }`), denied by default: without it
  the execution ends at the call. A timer due at or after the 5-second
  execution deadline ends the execution with a budget error instead of
  waiting, at most `MAX_PENDING_TIMERS` timers are pending, an exception from
  a timer callback ends the execution like one from `main`, and
  `AbortSignal.timeout` timers do not keep the loop alive. Four internal host
  imports (`timer_start`, `timer_clear`, `timer_take`, `timer_uncaught`) use
  host ABI 32; v35 and earlier keep rejecting the timer globals. Under v36 an
  arrow expression body in a `void` context discards its value, as in
  TypeScript.

- Async generators and `for await` under opt-in v36 (selector
  `async_generators`, K4.4 step 6). `async function*` declarations, generic
  instantiations and instance and static class methods return
  `AsyncGenerator<T, TReturn, TNext>` (also written `AsyncIterableIterator`,
  `AsyncIterable` or `AsyncIterator`); `next`, `return` and `throw` return
  promises, queue while the body runs and follow the ECMAScript request
  queue; `await`, `yield` and `return` work across `try`/`catch`/`finally`,
  and `yield` and `return` await their operands; `yield*` delegates to
  another async generator, forwarding `return()` and `throw()`. `for await`
  iterates async generators, arrays and synchronous generators with Node's
  job counts and closes them on `break`, `return` and throws. An async
  generator object is the frame of an internal body generator with a guest
  state record; awaits in bodies run through scheduled relay jobs, so
  suspended generators hold no live-frame slot and are collected. Under v36 a
  checked `as` may also contain `await`, and a `Math`/`Number` constant
  default types a parameter. No host import, value tag, layout or budget is
  added; `yield*` over other iterables, async generator expressions and
  `Symbol.asyncIterator` remain unsupported. Logs match Node 24.

- Opt-in v36 (pilot batch: checked declared fields, function fields, async
  generators and timers, in development) integrates its four streams. It
  inherits every v35 feature and declares its own host ABI 32
  (`TIMERS_VERSION`: the value17 readers and the timer operations) and value
  ABI/layout 17; the launcher advertises both. A v36 module relabelled as v35
  fails validation before any guest effect, and runtimes that do not list v36
  reject its declared profile name. The streams work together: timer callbacks
  and abort listeners may be held in function fields (rooted across
  collections), `for await` consumes async generators that sleep on timers,
  async generators yield function values, and declared reads of mixed records
  take optional chains (`varta-runtime` `v36_cross_stream_tests`, Node 24
  logs). `AbortSignal.any` joins the v36 abort prelude with DOM dependent-signal
  order, so `AbortSignal.any([signal, AbortSignal.timeout(ms)])` and the pilot's
  abortable sleep work. The conformance inventory pins v36: six v36 cases pass,
  the timer case with a reviewed per-case `time.timer` grant (the runner's new
  `grants` field, which admits no other capability).

- Function fields under opt-in v36 (`function_fields`): function values in
  record, interface and class fields, arrays, dictionaries, unions (one
  function type each) and `unknown`; calls through fields without `this`;
  optional and defaulted function-typed parameters such as
  `random: () => number = Math.random`; `typeof f === "function"` and
  truthiness narrowing; optional chains (`x?.p`, `x?.[i]`, `x?.m()`, `f?.()`);
  `Math.random` as a value with one identity; JSON omission of functions and
  a compile-time rejection of function-valued `toJSON` fields;
  `reject(reason)` with an `unknown` reason; `Promise.withResolvers<T>()`; and
  the v36 abort prelude's `addEventListener`/`removeEventListener`/`onabort`
  (registration order, `once`, capture identity). V35 programs compile to
  unchanged bytes (pinned digests); the conformance inventory gains three
  cases.

- Value ABI and layout 17 (`FUNCTION_FIELD_VALUE_VERSION`) for v36, owned by
  the function-fields stream: every value16 tag and layout keeps its meaning
  and the function tag becomes a reference kind of fields, elements and cells,
  read through closure envelopes. V36 binds thirteen value17 readers
  (`value_log_v11` … `bigint_from_value_v4`, host ABI 32), JSON schema 17 and
  exception channel v8; v35 keeps value16. The launcher advertises value ABI 17.

- Mixed records and JSON to `unknown` under opt-in v35 (selectors
  `mixed_index_records`, `unknown_json`). Object types
  may declare fields next to a string index signature (the pilot's
  `ProblemDocument`); they are dictionaries of the index type whose declared
  reads are typed and checked, so a value stored through a dynamic key or a
  plain alias throws `TypeError` instead of being reinterpreted. Dictionaries
  gain `for...in`, compound, logical and update assignments, spreads with a
  dictionary context, number keys (`ToPropertyKey`), `Record<string, T>`
  inference, and shared per-type read/write/delete functions that keep long
  runs within the function body budget. Classes may declare optional fields
  and redeclare an inherited field with the same type (`readonly cause?:
  unknown`), redefined after `super()` as in ES2022. `JSON.parse(text)` returns
  `unknown` (a graph of primitives, `unknown[]` and `Record<string, unknown>`),
  `varta.json.parse<T>` admits `unknown`, `JSON.parse(text) as T` is a checked
  parse at `T`, dictionary assertions are proven by the storage descriptor for
  any value type, and `typeof x === "object"` with a null test narrows
  `unknown` to `object` for `in`, `Object.keys`/`values`/`entries`/`hasOwn`
  and `for...in`, while `"key" in x` proves `x.key`. They run on v35's value
  layout 16 and JSON schema 16 and add no further value tag, layout, host
  import, schema version or budget.

- Engine Web types under opt-in v35 (selectors `web_headers`,
  `abort_signals`): `Headers`/`HeadersInit`, `AbortController`, `AbortSignal`
  and `DOMException` are typed guest preludes compiled with a program only when
  it names them, with Fetch/DOM validation, ordering and errors pinned to
  Node 24 and no host import or capability. Header iteration returns array
  snapshots; abort listeners, `AbortSignal.timeout` and `AbortSignal.any` are
  rejected with explicit diagnostics. V35 function literals may also omit
  trailing parameters of their contextual function type.

- Opt-in v35 (pilot batch: Web types, mixed records, async methods and frame
  liveness, in development) is registered. It inherits every v34 feature;
  runtimes that do not list v35 reject its declared profile name before any
  guest effect. Its selectors are `web_headers`, `abort_signals`,
  `mixed_index_records`, `unknown_json`, `async_methods`, `promise_values` and
  `frame_liveness`.

- Async class methods, async arrows and async function expressions under
  opt-in v35 (selector `async_methods`): instance methods with overrides and
  awaited `super.m()` calls, async closures that share their captured bindings
  across `await`, contextual or inferred `Promise<T>` results, optional,
  default and rest parameters in every async function, and `Promise<T>`
  inference in generic calls. Receivers and captured cells are exact frame
  roots; job order matches Node.

- Promise values under opt-in v35 (selector `promise_values`): promises in
  arrays, records, class fields, dictionaries, unions and `unknown`, compared
  by identity, `typeof` `"object"`, logged as `Promise { … }` and stringified
  as `{}`; `await` and `Promise.resolve` of `T | Promise<T>`; and
  `Promise.all`, `allSettled`, `race` and `any` over arrays of promises with
  Node's job order. `Promise.any` rejects with an `Error` (no
  `AggregateError`); a promise held by `unknown` is never adopted silently.

- Value layout 16 for v35 (`promise_values` foundation): value ABI 16 adds the
  promise value tag to every value15 layout, read by thirteen value16 host
  operations that mirror their value15 counterparts (host ABI 31), exception
  channel v7 and JSON schema 16. V35 programs behave exactly as under v34; v34
  modules keep host ABI 30, value15 and their imports, and swapped readers,
  channels or descriptor stamps are rejected before guest effects.

- Liveness-based continuation frames under opt-in v35 (selector
  `frame_liveness`): each `await`, `yield` and test-hook suspension point saves
  only the locals live across it (through the normal, throw and return
  continuations, closure-shared cells, handlers and finalizers), directly in
  frame segments of kind 3 without per-value frame cells; states reuse the same
  entries, and each segment's edge mask is the exact GC map of the state the
  frame is suspended at (zero while running and after completion). Frame
  catalog version 3 describes the per-state saved slots and the entry
  capacity; only v35 reads it, v33/v34 keep version 2 and their modules stay
  byte-identical, and a module relabelled across the two layouts fails
  admission. No host or value ABI changes. An async closure body's frame
  keeps its captured cells as shared state-0 slots, and an assignment to a
  closure-shared local keeps its cell live across a point. Measured with production budgets,
  an `await` costs about 1.6k fuel instead of 3.0k, a `yield` 1.1k instead of
  1.8k, and 141 instead of 57 suspended generators fit in one execution.

- `unknown` under opt-in v34 (selector `unknown_type`): a reference-union cell
  that holds any primitive, BigInt, record, array, native error or class
  instance (function values are rejected). Unnarrowed values admit only
  equality, nullish comparison, `typeof`, truthiness and logical operators,
  `instanceof`, `Array.isArray`, logging, `String`/template conversion and
  `JSON.stringify`; `typeof`, `=== null/undefined/<literal>` and `instanceof`
  narrow positive branches. `catch (e)` and `catch (e: unknown)` bindings are
  `unknown`, rethrowing preserves the caught value, and Error subclasses take
  `cause?: unknown`. Unknown values are exact GC roots in locals, fields,
  elements, closures and generator frames (`CaptureType::Unknown`).

- Checked assertions under opt-in v34 (selector `checked_assertions`): `x as T`
  is free for upcasts and otherwise tests the value by member tag, literal
  value, exact record descriptor, class family descriptor or primitive array
  element kind, throwing `TypeError("type assertion to <T> failed")` on
  mismatch; `x!` throws `TypeError` on `null`/`undefined`. Disjoint targets,
  distinct record or array types, arrays of non-primitive elements, function
  types and `<T>x` are rejected at compile time; `as const` is unchanged.

- Microtask scheduler and `Promise<T>` under opt-in v34 (selector `promises`,
  K4.4 step 3): one bounded FIFO microtask queue per execution, owned by the
  runtime and drained after `main`; every job is a scheduled continuation frame
  resumed only through `varta_resume` with generation-checked tokens, and the
  runtime keeps reaction serials, never guest references. `new Promise<T>`
  with a synchronous executor and one-time resolution, `Promise.resolve`,
  `Promise.reject`, `then`/`catch`/`finally` with handler-result adoption,
  promise adoption jobs and the self-resolution `TypeError`; job order matches
  Node event logs. Promises are guest-heap records without a value tag. Five
  internal scheduler imports join the v34 host ABI without a capability. A
  rejection still unhandled after the checkpoint terminates the execution
  with its reason; fuel, deadline and host-call ceilings cover every job and
  are never renewed, and exhaustion inside a job is fatal, never a rejection.
  Thenables other than internal promises, the combinators, promises in
  aggregates and async HTTP/tool handlers are not admitted.

- `async`/`await` under opt-in v34 (selector `async_functions`, K4.4 step 4):
  annotated top-level and generic `async function` declarations returning
  `Promise<T>` (including `async function main(): Promise<void>`), `await` of
  promises and plain values in normalized expression positions, `try`/`catch`/
  `finally` across `await`, rejection propagation and returned-promise
  adoption, over the same continuation frames. Async arrows, expressions and
  methods, `for await`, async generators, top-level `await` and `await` inside
  intrinsic value blocks fail with construct-specific diagnostics.

- V34 stream integration: `unknown` holds value15 dictionaries and wide records
  and reads them through the value15 operations; `x as Record<string, T>` for
  a primitive `T` tests the `Dictionary` tag and the value vector's element
  kind; promises are rejected from `unknown` with their own diagnostic;
  promise records, async frames and the scheduler run under value15
  descriptors, and recursive values (`JsonValue`, `Tree`) may be held in
  closure cells and generator/async frames through shallow back-edge catalog
  nodes. A flow join now keeps a refinement of an `unknown` binding that every
  path proves, so a narrowed `catch (e)` stays narrowed after a branch.

- Opt-in v34 (pilot batch: dynamic records, `unknown` and async, in
  development) is registered. It inherits every v33 feature and declares a new
  host ABI, so runtimes without the microtask scheduler reject its modules, and
  a new value ABI reserved for the batch's record and promise layouts; until a
  stream defines them, v34 keeps the v33 layouts, exception channel and JSON
  selectors. Its `index_signatures`, `recursive_types`, `wide_records`,
  `unknown_type`, `checked_assertions`, `promises` and `async_functions`
  selectors admit nothing yet.

- Value layout 15 under opt-in v34 (selector `dynamic_values`): v34 stamps and
  reads the value15 layouts, which keep every value13 tag and layout, through
  its own value15 readers (`value_log_v9`, `value_typeof_v8` and the other `_v8`
  value predicates, `value_is_error_class_v4`, `json_stringify_v14`,
  `json_parse_v16` with JSON schema 15, `primitive_to_string_v3`,
  `primitive_to_number_v3`, `bigint_from_value_v2`) and exception channel v6.
  V34 programs behave exactly as under v33; v33 and earlier keep value13.

- Wide records under opt-in v34 (selector `wide_records`): interfaces, object
  types, literals and classes with up to `value::MAX_WIDE_RECORD_FIELDS` (128)
  fields, a value15 layout with multiword presence words and a
  descriptor-trailing edge bitmap traced by the collector; JSON, presence,
  deletion, enumeration and rest destructuring behave as for narrow records.
  Earlier profiles keep the 32-field ceiling.

- Index signatures under opt-in v34 (selector `index_signatures`):
  `{ [key: string]: T }` and `Record<string, T>` as value15 dictionaries (tag
  13) with reads typed `T | undefined`, writes, `delete`, `in`,
  `Object.hasOwn`/`keys`/`values`/`entries`, literals, JSON stringify and
  checked parsing (JSON schema 15), ECMAScript own-key order and a
  `MAX_DICTIONARY_ENTRIES` bound. Mixed fixed/index object types are rejected.

- Recursive types under opt-in v34 (selector `recursive_types`): non-generic
  aliases and interfaces that refer to themselves through record fields, array
  elements or dictionary values, such as a `JsonValue` union and trees, with
  checked JSON parsing; non-productive cycles stay rejected.

- Synchronous generators under opt-in v33 (selector `generators`, K4.4 step 2):
  annotated top-level and generic `function*` declarations
  (`Generator<T, TReturn, TNext>`, with `void`/`undefined` defaults instead of
  `any`), `yield` in normalized expression positions and in `try`, `catch`,
  `finally` and `switch` case bodies, the `next`/`return`/`throw` protocol with
  ECMAScript state rules and the re-entry `TypeError`, `IteratorResult<T,
  TReturn>` records and `for...of` that closes the generator on `break`,
  `return` or a throw so its `finally` blocks run first. A generator object is
  its frame handle without a value tag (value ABI unchanged); it is resumed by
  a closed per-type step function and never through `varta_resume`. Frame
  catalog rows gain `generator` and the `Guest` input class. `yield*`, async
  generators, generator methods and generators inside aggregates or unions are
  rejected with diagnostics. Behavior matches pinned Node 24 logs.
- Runtime admission of the K4.4 frame surface under opt-in v33 (selector
  `resumable_frames`). Runtime preflight and artifact validation share one rule
  set: function catalog version 2 is read only for v33, `varta_resume` must be a
  closed guest function `(i64, i32, i64) -> i32` without tables or indirect
  calls, and each frame layout must agree with its creator, resume body and
  binding classes. V32 and earlier profiles reject every piece with a typed
  error before any guest effect. The runtime does not call the dispatcher yet;
  frame modules still come only from the test-only compiler hook.
- K4.4 suspension foundation (block 1), compiler-internal, admitted by no
  profile. It adds evaluation-order normalization of suspension points on IR and
  a resumable state machine over continuation frames, a separate managed kind
  that stores saved bindings in binding cells and shares captured closure cells.
  Frames have their own budgets and exact GC tracing, the exception channel is
  empty at every suspension, and the closed `varta_resume` dispatcher checks
  ownership, generation, reentrancy and inputs. Function catalog version 2
  carries the frame layouts; the version 1 reader rejects it. Unsupported
  positions fail with construct-specific diagnostics. Only compiler tests reach
  it, through a test-only lowering hook; existing modules and profiles are
  unchanged.
- Opt-in v32 (pilot batch, in development) adds nine composable features for
  the uarp-sdk pilot, each behind its own profile selector. V31 and earlier keep
  rejecting all of them. Host imports, value tags, layouts, host/value ABIs and
  production budgets are unchanged; the only versioned change is exception
  channel v5 for Error subclasses. The inventory adds one case per feature that
  a single-file case can test.
  - Module constants: top-level `const`/`export const` with a pure checked
    initializer subset and `as const` readonly bindings. Scalar constants fold to
    literals; reference values are evaluated once in dependency order before
    `main` and held in permanent collector roots.
  - Literal types: string, number and boolean literal types and their unions as
    compile-time value sets that erase to their base slots, with
    membership-checked assignability, `===`/`!==`/`||` narrowing and up to 4096
    members per set; all 356 string-literal aliases of the pilot `models.ts`
    resolve. `varta.json.parse<T>` rejects literal-typed targets.
  - Classes: typed and `readonly` fields, `public`/`protected`/`private` checks,
    constructors with a definite-initialization discipline, methods, getters,
    static methods, `extends` with `super(...)` and `super.m(...)`, nominal
    subtyping, `instanceof` and closed-world override dispatch. Instances are
    records with per-class descriptors and exact tracing. `#private`, static
    fields and setters remain rejected.
  - Module re-exports: `export { x as y } from`, `export type { … } from`,
    `export { type T } from` and `export * from` forward functions, constants,
    named types and classes without local bindings. Functions and constants share
    one value namespace; star exports follow ECMAScript rules (no `default`,
    explicit exports win, ambiguous names excluded and rejected only when named)
    and chain transitively. Namespace forms stay rejected.
  - Template literals: substitutions are evaluated once, left to right, converted
    with primitive ToString (canonical Number text, BigInt decimal text; literal
    types as their base) and joined by guest concatenation with exact UTF-16
    parts. Non-primitive substitutions (including class instances), tagged
    templates and `String.raw` fail with diagnostics. The source-complexity
    pre-pass rescans template text after each substitution, so template text and
    later doc comments no longer trip a false complexity error.
  - Default exports: `export default` declarations, bindings and constant
    expressions, `import x from`, `import x, { y } from` and `default` in export
    and re-export lists link as the ordinary name `default` (never forwarded by
    `export *`). Namespace imports are rejected with a diagnostic.
  - Interface inheritance and merging: `interface B extends A, C` builds the
    combined closed record from local, imported, aliased and generic record bases
    (`interface B<T> extends A<T>`) with checked redeclarations, and repeated
    non-generic interface declarations merge. Class bases, merging generic
    interfaces and interface/class merging (the pilot `UarpClient` pattern) are
    rejected with diagnostics.
  - Generics: generic interfaces, aliases and top-level functions by
    monomorphization, with defaults, `extends` constraints, explicit or inferred
    type arguments, `keyof`, `R[K]`, `Readonly`, `ReadonlyArray`, `Partial` and
    literal-keyed `Record`; one function per distinct instantiation within the
    existing function and body budgets. Generic functions and interfaces are
    default-exportable. `Record<string, V>`, `Promise<T>` and generic classes
    (including generic Error subclasses), methods and arrows stay rejected with
    reasons.
  - Error subclasses: classes may extend `Error`, `TypeError`, `RangeError` and
    `SyntaxError` with writable `message`/`name`/`cause` fields,
    `super(message, { cause })`, `new.target.name`, `toString()`, `instanceof`
    for the native family and every class in the chain (including downcast
    narrowing of caught values) and catch/finally/rethrow across modules.
    Instances stay class records with no new tag or layout; v32 selects
    exception channel v5 (`varta.exception.version_v5`) so the runtime reports
    an uncaught instance as `ThrownValue::ErrorInstance`. `unknown` causes,
    `stack`, JSON serialization of instances and `Error`-annotated instances
    remain rejected.

- Opt-in v33 (suspension and generators, in development) is registered. It
  inherits every v32 feature and declares a new host ABI, so runtimes without
  frame admission reject its modules; the value ABI, exception channel and JSON
  selectors are those of v32. Its `resumable_frames` and `generators` selectors
  admit nothing yet.

- Opt-in v31 adds named interface and type alias annotations, resolved per module
  and across imports, with `import type`, `{ type T }`, `export interface/type`
  and `export type { … }`. The project loader resolves `./x.js` specifiers to the
  `./x.ts` source for v31. Recursive, generic and `extends` declarations fail with
  diagnostics. Layouts, ABIs and limits are unchanged; the inventory adds one
  named-type case.

- Prepared runtime modules retain immutable verified function catalogs through
  startup, tool invocation and HTTP request execution. Stores own their module's
  metadata, reject replacement after instantiation starts and clear it on failure.
  Existing module-validation APIs and source-profile contracts remain compatible.

- Value14 codec groundwork adds catalog-checked Function tag12 and a Rust JSON
  graph encoder with function omission/null rules and bounded capture validation.
  Schema14 retains existing JSON input types; current source profiles/host imports
  stay unchanged pending function-aggregate integration.

- Allocation-free managed closure/environment/binding envelope views prepare
  versioned function-value transport, independently checked against emitted heap
  objects. Optional v30 capture catalogs add typed value/GC-mask validation,
  result-pair checks and bounded recursive closure inspection. Function-valued
  aggregate admission remains pending.

- Opt-in v30 adds optional-record deletion, primitive delete keys and array key
  handling, preserving canonical absence, order, aliases/GC and checked required
  fields. Object.entries returns fresh dense key/value pairs for closed records
  and typed arrays, preserving sparse presence and shared child identities. It
  reuses v29 layouts and advertises 33 source profiles. Object.keys/values/entries
  also enumerate String UTF16 units and primitive unions with guest-only helpers;
  keys/values evaluate ignored arguments before snapshots under v30. Object.is
  adds SameValue comparison, retaining NaN, signed-zero and reference identity
  semantics through rooted once-only argument evaluation. Checked Object.assign
  adds ordered record/array/String copies, shared identities, partial-write errors
  and preallocated field inference for fresh record literals. Existing objects
  retain declared storage; general dynamic properties and descriptors remain gaps.

- Opt-in v29 adds BigInt graphs, sparse arrays, optional records, primitive-union
  arithmetic/conversions, compound assignments and numeric updates, owned BigInt exceptions and recoverable JSON TypeError.
  ABI28/value13/schema13/channel4 preserve old profiles and exact roots; array
  searches use BigInt value equality and malformed graphs/resources stay fatal.
  The reviewed inventory then pinned v30 with 78 cases (67 supported, 11 known gaps),
  after promoting the unchanged coercive-width gap; primitive width/radix
  coercions preserve ToIndex/ToBigInt order and recoverable error classes.
  Primitive update ToNumeric preserves converted postfix values, NaN/presence
  semantics and checked numeric stores without widening mutable storage types.
  Primitive compound conversion handles String concatenation and numeric rules,
  bounded wide-union dispatch, correct mixed-numeric failure types and metered
  conversions, preserving once-only reads/RHS effects and checked stores.
  Ordinary primitive arithmetic, exponentiation, shifts and unary +/−/~ reuse
  conversion with scalar fast paths; unary plus retains native validation and
  a Number result domain while rejecting BigInt with TypeError.
  All eight primitive equality/ordering operators preserve exact BigInt, UTF16
  order, nullish/Boolean conversion and NaN semantics; native transport/fuel
  checks and scalar fast paths are covered by a 2048-comparison Node oracle.

- Opt-in v28 connects BigInt scalars to rooted binary leaves and pure ABI27
  arithmetic/conversion/comparison/width operations with guest fuel accounting,
  recoverable semantic errors and fatal malformed/resource errors. Value/schema12
  stay frozen; BigInt aggregate/union/throw/JSON and broader coercive built-ins
  remain pending. The reviewed corpus pins v28 with 46 cases (34 supported,
  12 known unsupported), including an explicit BigInt-array gap.

- Internal `varta-bigint` mathematical foundation provides immutable integers,
  parsing, arithmetic, shifts, bitwise operations, Number/radix conversions and
  signed/unsigned widths with caller-supplied bit/work budgets. It is now
  consumed by the v28 scalar profile. The
  binary leaf transport has canonical block/payload validation, budgeted
  import/export and a generated-GC fixture. Large shifts charge destination work;
  malformed binary transport never maps to a recoverable SyntaxError.

- Opt-in v27 infers one admitted union for uncontextual array elements, flattening
  existing unions and preserving source-order effects, exact Number bits, shallow
  reference identity and roots. Empty/all-hole peers use typed array evidence;
  explicit contexts, array invariance, shape/Function/opaque restrictions and
  production budgets persist. Shared ABI26/value12/schema12 requires rebuilt
  companions with the exact v27 profile list. The reviewed compatibility corpus
  closes its inference gap and grows to 40 cases (28 passes, 12 known unsupported).

- Developer-only `varta-conformance` pins an ECMAScript 2025/v27 checked-program
  inventory with source/dependency digests, isolated default-budget workers,
  bounded protocols and explicit pass/gap/regression/infrastructure/untested
  reporting. CI checks the reviewed baseline; strict mode exposes remaining gaps.
  The suite then recorded 28 passes, 12 known unsupported cases and 7 untested
  feature rows. Full Test262/script support remains planned CORE-15 work.

- Opt-in v26 adds checked Array.reduceRight with seeded/unseeded accumulator
  inference, descending live sparse traversal, recoverable empty-fold TypeError,
  callback/seed/ignored-argument ordering and rooted intermediate graphs. Existing
  reduce/profile admission remains unchanged; no new host imports, dependencies,
  ABI/layout versions or production budgets. Rebuild both companions for v26.

- Opt-in v25 adds checked Array.findIndex/findLast/findLastIndex with live sparse
  reads, forward/reverse snapshot traversal, cached matched values, receiver and
  ignored-argument evaluation, rooted callbacks and unchanged capability/fatal
  boundaries. Existing profiles reject these methods. The callback-before-thisArg
  lowering fix also prevents stale refinement reads in existing callback methods.
  Shared ABI26/value12/schema12 and production budgets remain unchanged; rebuild
  both native companions for exact v25 profile negotiation.

- V24 specializes stable callback parameters at proven calls, propagating symbolic
  inputs through forwarding, const aliases and captures. Direct/nested parameter
  replacements retain unknown reentry. Default/rest/receiver argument mapping,
  borrowed guard/loop effects and shared proof-depth limits preserve existing
  call/root/status contracts without ABI/layout/production-budget changes.

- V24 bounds local-cell effects using immutable function provenance and transitive
  closure summaries. Known calls preserve unrelated captured facts; module calls
  account for supplied callback proofs. Argument effects, unknown/self/parameter
  calls and mutable/returned functions retain conservative invalidation. Checked
  non-reentrant member intrinsics preserve binding facts without promising
  unchanged heap contents. No ABI/layout/production-budget or runtime dependency change.

- V24 tracks reachable break/continue facts by loop, switch and label target.
  Updates/do-while tests join their actual inputs; normal loop/switch/label exits
  combine false tests and matching breaks. Finally writes transform pending exits,
  and overriding returns/throws/branches cancel old completions. Repeated-part
  analysis after one-time for initializers fixes stale second-test assumptions.
  Physical control routing, ABI26/value12/schema12 and production limits stay intact.

- V24 infers local union types after successful stores, including destructuring,
  compound/update and logical assignments. Normal if/ternary branches join their
  resulting types; early return/throw paths do not erase surviving facts. Skipped
  defaults/RHS, switch entries/fallthrough, labels, loop continues and exceptional
  prefixes cannot leak a later store's fact. Declared storage, prior profiles,
  ABI26/value12/schema12 and execution budgets are unchanged.

- V24 allows guarded writes through per-binding revision tracking, branch joins,
  loop effect analysis and conservative invalidation after possible user calls.
  Mutable captures retain declared cell types; const captures inherit stable
  narrowed types through nested closures. Compiler-only swc_ecma_visit29.0.0 adds
  bounded AST traversal. Runtime distribution, ABI26/value12/schema12, capabilities
  and production limits remain unchanged; older profiles keep readonly guards.

- Opt-in `checked-f64-utf16-v24` adds truth-based direct-local narrowing, compound
  guards and &&/||/?? RHS refinement, including while/for bodies and for updates.
  False branches retain potentially false Number/String/Boolean members. Scoped
  refinements use the write/capture analysis above; v23 keeps
  its earlier source admission. ABI26/value12/schema12, imports and limits are
  shared unchanged; native negotiation adds v24 to the supported profile list.

- V23 integrates guest ToBoolean into conditions, loops, ternaries, !, logical
  assignment tests and existing predicate callbacks. Operand-valued &&/||/?? cache
  the original left value, narrow selected branch types and preserve identity,
  signed zero and short-circuit effects without native truth/nullish imports.
  Guest roots, fatal limits and older profiles retain their contracts; no ABI,
  dependency or resource-limit change.

- V23 adds direct Boolean conversion over admitted ordinary values and coercing
  global isFinite/isNaN over primitives/unions. Exact guest ToBoolean uses no host
  call or allocation; numeric predicates reuse existing Number conversion only
  for non-Number arguments. Cached effects/roots, ordinary binding precedence,
  status and fatal limits are retained, with no ABI/dependency/resource changes.

- Opt-in `checked-f64-utf16-v23` / ABI26 adds direct global and Number.parseInt /
  parseFloat calls on primitives/unions. Longest-prefix grammar, exact leading
  whitespace, signed Int32 radix conversion, hexadecimal detection, signed zero
  and bounded nearest/even integer rounding preserve effects, roots/status and
  fatal budgets. Each present input uses one capability-free host call; no result
  buffer, dependency, value/schema12 or resource/container limit changes.

- Opt-in `checked-f64-utf16-v22` / ABI25 adds direct Number.toFixed,
  toExponential and toPrecision with bounded exact rounding, primitive precision
  conversion, method-specific RangeError order and owned UTF-16 results. Direct
  Number.valueOf preserves the original bits and ignored effects without a host
  call. Value/schema12, status, resource limits and container formats are retained.

- Opt-in `checked-f64-utf16-v21` / ABI24 adds direct Number.toString for bases2–36,
  cached primitive radix arguments and catchable RangeError in the specified order.
  Bounded exact intervals choose shortest nondecimal text using direct runtime
  num-bigint0.4.8 (already locked via the compiler); decimal text reuses ryu-js.
  Owned results, roots/status, fatal budgets and source-free packaging retain
  value/schema12 and existing resource/container boundaries.
- Opt-in `checked-f64-utf16-v20` / ABI23 adds primitive Number(...) conversion
  and whole numeric String parsing: exact whitespace/grammar, decimal/exponent and
  binary/octal/hexadecimal prefixes, nearest/even rounding, signed zero and NaN/
  infinity rules. Cached effects/roots, status/buffers, fatal limits and source-free
  packaging are retained, with unchanged value/schema12 and no dependency change.
- Opt-in `checked-f64-utf16-v19` / ABI22 adds primitive String(...) conversion,
  including admitted unions, absent/ignored argument rules, canonical Number text
  and exact UTF-16. Checked cells transfer through the existing result buffer into
  owned strings; status, roots, fatal limits and lexical bindings are retained.
  Source-free packaging negotiates v19/22; value/schema12 and limits are unchanged.
- Opt-in `checked-f64-utf16-v18` / ABI21 adds canonical ECMAScript Number output
  for scalar and union logs: shortest decimal/scientific notation, zero and
  nonfinite rules. Existing ryu-js1.0.3 formats numbers; old profiles retain their
  text. Exact registry/signature gates, budgets/status, roots and source-free
  native packaging are covered, with unchanged JSON/value/schema/limits.
- Opt-in `checked-f64-utf16-v17` / ABI20 adds direct Math.random with fresh OS
  entropy, exact 53-bit samples in [0,1), ignored argument effects and fatal
  entropy/budget failures. Runtime-only pinned getrandom 0.3.4 supplies portable
  entropy; no seed/cache is persisted. Value/schema/container versions and limits
  stay unchanged; native packaging negotiates v17/20.
- Opt-in `checked-f64-utf16-v16` / ABI19 adds nineteen unary native Math methods
  and atan2 through a shared typed pure registry. Required zero/NaN/infinity/domain
  rules precede platform-approximated finite math; one-call/deadline accounting,
  status, helper purity and old-profile contracts are preserved. Value/schema12,
  container formats and limits are unchanged; native packaging negotiates v16/19.
- Checked variadic v15 `Math.hypot` uses exact power-of-two scaling, compensated
  squares/sums and root refinement in guest code. All arguments execute before
  Infinity/NaN selection; extreme values, subnormals, near-overflow high-precision
  goldens, GC/pairs and artifact/permission paths are tested without new ABI/budgets.
- Checked v15 `Math.f16round` rounds binary64 directly to binary16 ties-to-even
  using guest instructions. Signed zero, subnormals, overflow and halfway neighbors
  are verified against an independent bit oracle without binary32 double rounding,
  new imports, ABI versions, dependencies or increased execution limits.
- Checked v15 `Math.pow(base, exponent)` shares the existing exponentiation
  import, Number edge cases, finite approximation and one-call/deadline budgets.
  Argument order, host-free-helper rejection and operator/method agreement are tested.
- All eight Math numeric constants and eight Number constants in v15, with exact
  binary64 values and dot/literal-key reads. Lexical shadows retain ordinary
  properties; built-in writes have a read-only diagnostic. Guest constants add
  no host imports, allocation, ABI changes or limits.
- Direct v15 `Number.isFinite/isNaN/isInteger/isSafeInteger` predicates accept
  admitted values and unnarrowed unions without coercion. Non-Numbers/missing
  arguments return false; ignored extra arguments retain all effects and errors.
  Guest-only execution preserves status, pairs/roots and existing ABI/budgets.
- Checked `Math.min/max`, `sign`, `fround`, `clz32` and `imul` in v15. Variadic
  extrema preserve argument order and NaN/signed-zero rules; binary32 rounding
  and exact modulo-32-bit conversions/products use guest instructions. No new
  host import, capability, ABI version or runtime budget.
- Direct checked `Math.abs`, `floor`, `ceil`, `trunc`, `round` and `sqrt` calls
  in v15, each with one narrowed Number argument. Guest instructions preserve
  signed-zero/nonfinite rules and rounding toward positive infinity on ties,
  without new imports, permissions, helper exports, ABI versions or limits.
- Opt-in `checked-f64-utf16-v15` / host ABI v18 adds Number `**` and `**=` through
  the pure typed `number_power` import. Explicit NaN/zero/Infinity/odd-integer
  rules precede implementation-approximated finite power, preserving status and
  one-call/deadline budgets. Value/schema v12 and container formats are retained;
  older profiles reject the new operators/import and native negotiation includes v15.
- Checked `&=`, `|=`, `^=`, `<<=`, `>>=` and `>>>=` in v12+ for mutable variables,
  static/dynamic record fields and numeric arrays, plus length in v14. Retain
  old values before RHS effects, validate actual Number operands, reuse full-range
  bitwise conversion and store computed results through existing checked references.
- Number-only `&`, `|`, `^`, `~`, `<<`, `>>` and `>>>` in v12+, with full-range
  ToInt32/ToUint32 semantics, signed/unsigned Number results and masked shifts.
  Conversion is inline, host-free and nontrapping, with exhaustive exponent/
  seeded-payload checks and unchanged union-narrowing/profile/resource boundaries.
- Checked prefix/postfix `++` and `--` in v12+ for mutable variables, static/dynamic
  record fields and numeric arrays, plus length in v14. Require an actual Number,
  store through one prepared reference, then return the exact old/new value;
  preserve partial effects, captured loop cells, roots and fatal boundaries.
- Checked `&&=`, `||=` and `??=` in v12+ for mutable variables, static/dynamic
  record fields and numeric arrays, plus length in v14. RHS evaluation and checked
  stores run only in the selected branch, retaining original values/references,
  presence, result types and rooted partial-chain/GC/finalizer behavior.
- Dynamic record `+=`, `-=`, `*=`, `/=` and `%=` in v12+, retaining the original
  receiver/key and old value across RHS effects, with checked computed results,
  exact selected-field stores and existing read-union/absence/prototype boundaries.
- Checked `+=`, `-=`, `*=`, `/=` and `%=` in v12+ for locals, static fields
  and numeric array references, with cached old values, matching Number/String
  guards, computed results, rooted nested effects and existing v14 length checks.
- Simple local, dot-field and numeric-array assignment expressions in v12+, with
  shared prepared references, original RHS types/values, full result/function
  ownership and v14 length-expression checks.
- Computed record assignment expressions/chains in v12+, returning the original
  RHS type/value with prepared references, static literal checks, rooted temporary
  floors and preserved inner writes on outer failure.
- Typed computed String assignment statements on closed records in v12+, with
  prepared receiver/key roots, exact selected-field checks, Error family guards,
  optional presence/order and catchable failures after RHS effects.
- Typed computed String reads on closed records in v12+, with inferred field/Undefined
  unions, presence-aware UTF-16 lookup, preserved payload identity and rooted operand
  effects. Unsupported inherited lookups raise TypeError; prototype access stays closed.
- Dynamic String own keys for sparse arrays in v13+ and Error families in v12+,
  preserving canonical indices/length, holes versus explicit Undefined and own
  message presence, argument effects, exact roots and older profile boundaries.

- Checked TypeScript-to-Wasm compilation, local module loading, source diagnostics
  and explicit, versioned language profiles.
- Opt-in binary64 numbers, UTF-16 strings, typed unions, optional fields, bounded
  arrays, checked JSON, recoverable exceptions, typed functions and lexical closures.
- Array callbacks, optional/default/rest parameters, spread calls, and shared
  object/array destructuring for parameters, local declarations and assignments.
- Expression-valued destructuring assignments and checked record-field/array-element
  targets, including defaults and rest. V14 adds indexed extension and writable length.
- Intrinsic array for-of in v12+, with binding/assignment patterns, per-iteration
  lexical captures, live mutation, sparse Undefined visits and finally cleanup.
- Checked do-while in v12+, with trailing-condition continue, finalizer routing,
  capture/root cleanup and guaranteed-first-body completion/return inference.
- Statement labels and named break/continue in v12+, including loop aliases,
  exact finalizer/scope routing and target-aware completion inference.
- Object.keys for closed records in v12+, with presence-aware JS key ordering,
  fresh dense UTF-16 string-array results, nullable checks and rooted snapshots.
- Object.values for closed records in v12+, with inferred homogeneous/union
  arrays, shared own-property ordering, rooted payload preparation and shallow identity.
- Object.keys/Object.values for bounded arrays in v12+, including sparse own-index
  enumeration, exact element types and rooted host-free decimal key creation.
- Catch object binding patterns in v12+ for name/message, with aliases/defaults,
  TDZ, rooted initialization and existing exception/finalizer routing.
- String.slice/substring in v12+, with bounded Number/Undefined normalization,
  exact UTF-16 ranges, owned result storage and preserved Array.slice dispatch.
- String.includes/startsWith/endsWith in v12+, with exact UTF-16 matching,
  bounded positions, allocation-free helpers and unchanged Array.includes rules.
- String.indexOf/lastIndexOf in v12+, with UTF-16 code-unit indices,
  method-specific position normalization and allocation-free helpers.
- Array.lastIndexOf, Array.reverse, and Array.fill in resizable-array profiles,
  with tagged-value matching/bounds, in-place mutation, sparse handling and
  ownership preservation.
- String.trim/trimStart/trimEnd in v12+, with exact UTF-16 whitespace
  removal, surrogate/NUL preservation, and owned result storage.
- Dynamic String keys for Object.hasOwn on closed records in v12+, with exact
  UTF-16 decision ordering, rooted argument evaluation and presence-only lookup.
- Capability-checked host APIs, independent host policy ceilings, programmatic
  operation results, and bounded execution with managed guest memory.
- Checked synchronous HTTP/1 handlers in the default language profile.
- Primitive tool discovery and checked named JSON invocation.
- Portable Wasm artifacts and native executables with an independent execution-only
  runtime companion, without shipping the TypeScript compiler.
- Contributor and security policies, an MIT license, and GitHub issue/PR templates.

### Changed

- `fuel::JSON_STRINGIFY_VALUE` is 5,500 (was 1,500; owner decision): the
  published calibration fitted to v38's linked encoder (the largest
  per-value need of the measured shapes, a logged promise, 5,026, rounded
  up), so a host serialization or value-logging import rated at the v38
  rates never costs less than the linked encoder. V38 links both, so the
  rate charges only test builds that import them and rated earlier
  profiles; the generated fair-fuel table lists it.
- V38 per-turn fuel default (owner decision): `budget::TURN_FUEL` =
  8,000,000 fuel per turn (`Budgets::TURN`), from the measured console turns
  at the fair host-fuel rates; a manifest can only lower it and only a host
  ceiling can raise it. v1–v37 keep `abi::FUEL` per execution. The generated
  limits table lists both.
- Pilot pin: uarp-sdk 0.13.2 (owner decision; 65 of 65 files check under v38,
  sources unmodified).

- Unhandled-rejection reports (v34 and v35 scheduler) hold bounded host
  memory: each reason is copied out of guest memory only up to 256 UTF-16 code
  units per string field and rendered to at most 2048 bytes, at most 16
  outstanding reports keep a rendered reason and every other report keeps one
  tracking bit. The termination diagnostic marks a cut reason with
  `[reason truncated]` and names an earliest remaining report whose reason was
  not kept as `<reason not retained>`; `SchedulerError::UnhandledRejection`
  carries a `RejectionReason`. Guest-observable behaviour and the termination
  itself are unchanged.

- The module graph admits 256 unique modules (was 64) and 2048 import and
  re-export declarations (was 256) in every profile. The active-path depth, the
  per-file and aggregate source byte limits and the user-function limit are
  unchanged. The pilot package has 65 modules and about 370 relative
  declarations, so its entry graph exceeded the earlier limits.

- Source complexity limits bound recursion instead of width: the per-file token
  count is removed (byte limits remain), and list commas in `[...]`/`{...}` and
  top-level union/intersection members of a type alias no longer count toward
  the 256-token chain limit. Nesting, expression chains and call arguments keep
  their existing limits.
- In v31 a module-local type named like an Error family (for example an API
  `Error` model) shadows the built-in type name in annotations, as in TypeScript.

- Documentation is restructured: a short README orientation page, a
  [language guide](docs/language-guide.md), per-profile
  [implementation rules](docs/profile-rules.md), a [history](docs/history.md) of
  completed increments and a rules-only `CLAUDE.md`. Contract text moved
  verbatim. The development plan records the research thesis and the uarp-sdk
  pilot. Version and limit tables in `docs/architecture.md` and the profile indexes
  are generated from the registries and checked by a workspace test.

### Fixed

- Fuel is identical on every host. Wasmtime meters instantiation's
  data-segment copy, which copy-on-write memory images skipped on Linux
  (memfd) and charged over 16 KiB pages on macOS arm64, so a v1–v37 module
  consumed host-dependent fuel (a one-byte segment cost 16,385 fuel on macOS
  and 0 on Linux). Owner decision: instantiation is no longer charged to the
  guest in any profile, as v38 already did, so every artifact that fit its
  budget still fits on every platform; Linux fuel is unchanged and macOS fuel
  only goes down. A module with a Wasm start function (possible before the
  exception profiles; the compiler never emits one) runs guest code during
  instantiation and keeps paying for it; the runtime disables copy-on-write
  images (`Config::memory_init_cow(false)`) so that charge is one fuel per
  segment byte everywhere (they save no measurable time for a fresh store).
  `fuel_determinism_tests` pins exact fuel of representative programs, that
  instantiation leaves guest fuel untouched, and that compilation is
  byte-identical across runs. Emitted bytes are unchanged. The codegen-v2
  keyed view mount speedup is re-pinned from 2.0 to 1.99 (v37 164,741 against
  v38 82,600; 2.0 held only with the macOS page charge).
- V38 application instances order network completions by their arrival
  among host tasks and due timers (they drive their transfers before every
  choice), so a stream of host events or timers no longer starves a
  completion; scripted sessions derive the order from arrival numbers and
  loop time. The pending-request limit fails before the permission broker
  decides, and a timer due beyond a bounded run's deadline reports
  `TimerDeadline` while a request is in flight. V37 is unchanged.

- V38 wave-3a integration: a fair-fuel charge that empties the fuel inside
  `host::input` or `host::result*` of the event, network-text and KV imports
  is the typed fuel-budget error (`HostFailure::into_trap`), no longer an
  ad-hoc host error message.

- V38 wave-2 integration: strict record-union matching (`web_bytes`) over
  more than 16 members tests a member already held by the cell as a flat
  guarded sequence, as strict members' discriminant dispatch does, instead of
  a chain nesting with the member count; `bigint_from_value_v5`, the value18
  BigInt conversion, is linked like the other BigInt operations (primitive and
  BigInt cells keep their value17 layout), so a v38 host still implements no
  BigInt import.

- V38 `charAt` and string index reads of a non-ASCII unit copy the unit into a
  one-unit string in the guest instead of calling the linked
  `string_char_at16`, which copied the whole string per call (about 670 fuel
  on a 4-unit string, 23,600 on a 1002-unit one, more than v37's host call).
  A non-ASCII read now costs about 100 fuel at any length; a Markdown-like
  block parser of one chat reply fell from 80,042 to 59,276 fuel.
- `varta dev --profile-fuel` profiles programs with checked JSON, tools or
  HTTP: the host-free value, tool and HTTP ABI helpers and their call graph
  stay uninstrumented (validation rejected their probes as host imports), and
  their fuel counts toward the caller, so the profile still equals the
  ordinary run's fuel.
- V38 keeps the narrowing of parameters and `let` bindings inside closures
  created after their last assignment (TypeScript 5.4), for arrows,
  function expressions and TSX hole getters: `case "code": return
  <code>{n.text}</code>` now compiles. An assignment after the closure, in
  any nested function or in a loop around the closure withholds it; hoisted
  function declarations never take it. V1–v37 are unchanged.

- V38 `varta dev --view-script` runs on virtual time: a scripted input is
  performed only when no task is runnable (records, arrived completions, due
  timers, and zero-delay timers with their chains in Node's order), so a node
  created in a `setTimeout(…, 0)` turn can be clicked by the next step. The
  new `{"input":"wait","ms":N}` step advances virtual time and fires the
  timers due by then; nothing waits for the wall clock, a timer no wait
  reaches never fires, and one step settles within `Script::MAX_STEP_TURNS`
  turns. Root cause of the run-to-run differences: `run_view` decided each
  timer's dueness by comparing its wall-clock target with `Instant::now()`
  after every turn, so a 1 ms timer fired before or after the next input
  depending on host speed. Scripted sessions are now deterministic (25-run
  runtime and 20-run CLI tests compare turns, output and snapshot).

- V38 accessible names follow the accname 1.2 steps the L0 subset admits:
  `aria-hidden="true"`, `display: none` and closed-dialog descendants no
  longer enter a name from content (`button "Reasoning"`, not
  `"◇Reasoning▸"`), `aria-labelledby` and `label` traversals include hidden
  content only when the referenced node is hidden, blank `aria-label` is
  ignored, embedded fields give their value, names are flat strings and only
  ARIA "name from contents" roles take their content (a `listitem` no longer
  has a name). The headless snapshot omits empty names.
- V38 resolves `./x.js` to `x.ts` or `x.tsx` (both existing is an ambiguous
  import error, neither names both candidates) and `./x.jsx` to `x.tsx`, as
  TypeScript's extension substitution does; probes use the pinned no-follow
  root. V31–v37 keep `.js` → `.ts`.

- Dispatch Array.lastIndexOf to reverse search instead of Array.indexOf. Omitted
  positions start at the final element; explicit undefined/NaN select zero,
  positive infinity clamps to the final index and positions below -length miss.
- Generate valid sparse Array.reverse helpers by retaining the i64 array handle
  for bitmap lookup; keep payloads, presence and identity together in v13/v14.
- Preserve U+0085 in trim/trimStart/trimEnd: it is not ECMAScript WhiteSpace or a
  LineTerminator. Tests cover all 25 admitted whitespace/line-terminator units.
- Restore v13 rejection of writable length/deletion and fatal indexed extension
  checks and add source-free regression coverage.

### Compatibility and limits

- The default profile retains its checked i32/UTF-8 semantics; newer language
  features require their documented opt-in profiles.
- Native packages target the build machine's OS and CPU. CI covers Linux and
  macOS; Windows and cross-target packaging remain unverified.
- VARTA3 packages contain portable Wasm. Legacy VARTA1/VARTA2 and native AOT
  payloads are rejected and must be rebuilt.
- Full ECMAScript/TypeScript and Node/npm compatibility, asynchronous execution,
  MCP bridges and artifact signing remain planned.

See the [language guide](docs/language-guide.md) for supported features and the
[development plan](docs/development-plan.md) for remaining work.
