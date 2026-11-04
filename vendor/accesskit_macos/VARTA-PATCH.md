# Varta AccessKit macOS patch

This is `accesskit_macos` 0.27.2 from its crates.io release, under the supplied
Apache-2.0 OR MIT licenses. Original release archive SHA-256:
`123828ceeec755b05255e3976cb0852993202de98a0626c97dd8d34246991f84`.
Upstream: <https://github.com/AccessKit/accesskit>, release commit
`f3026222559c7f5f46d8f25cb7404de68eb149f0`, package path `adapters/macos`.
The release archive omits the repository-root license files; both are included
here verbatim from that exact commit. The normalized manifest adds only the
existing pinned `sha2` 0.10.9 dependency. README and tests are otherwise copied
unchanged. Registry bookkeeping and the independent lockfile are omitted. `VARTA-PATCH.diff` records all source differences.

The original event generator announces updated node labels individually. It does
not defer busy regions, combine atomic content, or announce live value changes.
Varta supplies the admitted alert/status atomic defaults, explicit live settings,
busy containers and masked field values in its native tree. This patch consumes
those ordinary AccessKit properties without changing its public API:

- `src/live.rs` is a safe, iterative, deterministic snapshot planner. It separates
  announcement triggers from complete atomic message content. A changed enabled
  channel triggers its nearest atomic owner, whose message includes the entire
  visible subtree, including nested live/off content and its author label. Off
  changes alone do not trigger a message. Nested atomic owners handle their own
  triggers. SHA-256 digests track delivered trigger content per priority channel;
  busy periods retain those fixed-size fingerprints instead of historical text.
  Per-source-part fingerprints distinguish text changes/additions from removals;
  removals alone cannot announce remaining content. Owner/channel fingerprints
  coalesce same-content replacement within a presentation and busy cancellation.
  Relative reorders identify participating parts, so an unrelated unchanged
  assertive channel does not upgrade a polite reorder. Order caches retain IDs
  only, pruning removed sources.
  Release emits the current complete message once. When several channels change
  for one owner, only the highest changed priority is emitted. An unchanged
  assertive channel does not escalate a polite update. Empty/removal updates,
  unchanged repaints and cancellations do not replay old text. Field text runs
  are omitted because their owner's value already contains the rendered text;
  Varta password values contain grapheme masks before reaching the adapter.
  For content-derived parent names, Varta retains the resolved label and uses
  the existing `labelled_by` relation to its semantic children. The planner
  omits that derived-label copy, retaining actual author labels and leaf names.
- `src/context.rs` primes the planner on activation. Existing content starts as a
  baseline, while initially busy regions can announce their ready content later.
- `src/adapter.rs` processes live changes once after a complete tree update.
- `src/event.rs` replaces the old per-node announcement callbacks with that plan,
  feeding the existing AppKit announcement queue and its medium/high priorities.
  Changes to text, name relations, role, live/busy/atomic/hidden state or topology
  trigger planning;
  geometry/selection-only updates do not. Existing focus/value/selection events
  keep their original paths. No new platform `unsafe` block is added.
- `src/lib.rs` registers the module; `src/node.rs` removes the obsolete private
  label convenience method formerly used only by the replaced callbacks.

The planner is used only by Varta's bounded native observer tree. Projected
labels/values occur once in a flat fragment buffer, with range indexes for atomic
subtrees. Complete messages are built only on delivery. The existing fixed
observer allowance also preflights possible full atomic-message work, including
external label relations and field/mask capacity, before native projection.
Over-limit trees use the recoverable observer-refusal path without changing guest
admission. Hidden nodes are absent and runs have their own fixed ceiling. The
persistent baseline contains only keys and fixed-size digests; historical strings
cannot accumulate as busy regions alternate large content. These are structural
bounds of Varta's integration, not a complete RSS/CPU guarantee or a guarantee for
arbitrary third-party AccessKit producers.

Desktop regressions compile this exact `src/live.rs` file and apply real Varta
publications through `accesskit_consumer::Tree`. They cover atomic status updates,
assertive alerts, complete nested live/off content, nested atomic ownership,
changed-channel priority, root/ancestor/descendant busy states, initial busy
state, cancellation through empty content, empty/removal/replacement updates, password
masks, and activation baselines. They do not substitute for VoiceOver or other
platform acceptance. Reassess/remove the patch when an upstream release supplies
equivalent behavior and passes those regressions. Do not edit the registry cache.
