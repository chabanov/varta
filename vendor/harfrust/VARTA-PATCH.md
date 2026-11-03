# Varta HarfRust patch

HarfRust 0.12.0 is copied from its checksum-pinned crates.io release under the
included MIT license. Upstream: https://github.com/harfbuzz/harfrust ; package:
https://crates.io/crates/harfrust/0.12.0 . Original archive SHA-256:
`c03d949a14aa089bbb282f7dd76a498a7f684428e4257202efc119ec010376f9`.

`VARTA-PATCH.diff` records changes to six upstream source files:

- `src/hb/face.rs` adds optional `ShapeOptions::limits` for intermediate buffer
  slots and charged operations. These can only reduce the built-in safety
  ceilings. The result records operations charged before upstream resets its
  counters. A rejected charge makes the result unsuccessful; zero remaining is
  valid when all charged work has completed.
- `src/hb/buffer.rs` adds explicit completion/capacity/operation inspection and
  bounded clearing of reusable input arrays. Growth checks the requested length
  even when existing storage could hold it, uses fallible reservation and caps
  geometric requested capacity before resizing. Existing spare capacity is used
  without unnecessary growth. A shared checked `charge_ops` reserves the whole
  amount before corresponding buffer mutations/allocations; refusal is sticky,
  non-overrunning and does not debit the unfunded batch. The final statistic
  resets on clear.
- `src/hb/ot_layout_gsubgpos.rs` precharges recursive lookup work before cursor
  movement/recursion instead of checking after movement.
- `src/hb/kerning.rs` and `src/hb/aat/layout_kerx_table.rs` precharge each state
  transition, including its terminal transition when executed, and remove forced
  advancement after exhaustion.
- `src/hb/aat/layout_morx_table.rs` precharges transitions and current/marked
  insertion counts, refusing without forced progress or overspending.

Varta's Parley patch configures limits before input reservation and shaping,
checks completion before copying glyphs into a layout, shares output/operation
budgets across runs, and discards refused/oversized reusable buffers. Source
reservation counts Unicode scalars rather than UTF-8 bytes. No partial result
is exposed as a successful Varta measurement or rendered frame.

The capacity statistics cover the glyph-info and position arrays only. They do
not measure allocator metadata, overlap during reallocations, other buffers,
font/analysis/plan caches, total process RSS or wall-clock time. Upstream's
charged-operation counter is not a count of every CPU instruction or parser
step. All existing direct debits now use checked pre-charge accounting, so the
reported charged work cannot exceed its allowance. An unaffordable batch is not
executed or charged. Font validation, plan construction, Unicode analysis and
other unmetered work still need separate accounting. This is not a complete process-memory or deadline boundary.

The normalized manifest, source, README and license are otherwise unchanged.
The independent release lockfile and registry bookkeeping were omitted. The root
workspace lockfile retains dependency versions and selects this local source.
Archive/file comparison is recorded in `target/shaping-budget-vendor.log`;
`VARTA-PATCH.md`/`.diff` are Varta documentation. Do not edit the registry cache.

Tests exercise exact buffer/output boundaries, aggregate multi-script output and
operation refusal, maximum ordinary input, strong synthetic OpenType expansion,
cache release and successful reuse after refusal. Original AAT state-machine
fonts cover current/marked insertion, legacy kern, kerx and a nonadvancing loop;
all smaller budgets refuse and exact observed budgets complete successfully. Reassess/remove this patch when
an upstream update offers equivalent request limits, completion status and
capacity behavior and passes the native regression matrix.
