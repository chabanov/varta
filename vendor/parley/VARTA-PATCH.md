# Varta Parley patch

This is Parley 0.11.1 from the crates.io release, used under its supplied
Apache-2.0 OR MIT licenses. Both license files are retained. Upstream repository:
https://github.com/linebender/parley ; release package:
https://crates.io/crates/parley/0.11.1 .

Original crates.io package SHA-256 (from the preceding lockfile):
`22d2ff88bd3f7d68d1d9b09c7e6209f9a8e8c05088295140a2bcf2e9b17038c5`.

`VARTA-PATCH.diff` records all source changes relative to that release:

- `src/layout/data.rs`: cluster text offsets use `u32` instead of `u16` so a
  valid 98304-byte UTF-8 field run retains its source positions. Cluster glyph
  counts also use `u32`, with `u32::MAX` reserved for the inline-single-glyph
  representation; counts 255/256 and larger must not collide or wrap.
- `src/layout/cluster.rs`: glyph iteration recognizes that inline sentinel.
- `src/shape/mod.rs`: the contributing-character counter uses `usize` to support
  whole graphemes longer than 255 scalars without debug overflow/release wrapping.
- `src/analysis/cluster.rs`: coverage counts use `usize`, and font mapping scans
  every character rather than a fixed 32-element scratch prefix. A second pure
  nominal-mapping pass updates glyph IDs only for a better candidate, retaining
  prior IDs for a losing font without allocating a larger temporary buffer.
  Pairwise normalization's independently bounded two-character representation
  is unchanged.
- `src/context.rs` configures aggregate layout output/operation and intermediate/
  cached-array limits and exposes explicit completion/capacity statistics.
  An additional operation ceiling lets the Varta adapter share a view-request
  allowance across builds without raising any per-layout limit. Cloning keeps
  both configured policy and the ceiling while discarding scratch state.
- `src/builder.rs` discards incomplete layout contents. Shaping reserves scalar
  slots, applies the HarfRust request limits, checks completion before copying
  glyphs, accounts across runs, and drops refused/oversized reusable buffers.

The original offset failure caused invalid UTF-8 slice panics. Large combining
clusters also lost glyphs at the old inline sentinel, overflowed a character
counter and could select an incomplete fallback font. Varta still bounds source
input before shaping and retains the same glyph/outline/aggregate paint budgets.
The patch preserves complete shaping context; it does not truncate text, split
graphemes or raise public shaper/guest/work limits. The bounded request path uses the companion `../harfrust` patch. Array/operation
statistics are partial resource accounting, not a complete bound on dependency
caches, parser/analysis CPU, allocator overlap or RSS.

The normalized Cargo manifest, upstream source, README and licenses were copied
from the release. Registry bookkeeping and its independent lockfile were omitted.
`VARTA-PATCH.md` and `VARTA-PATCH.diff` are Varta-specific documentation. Only the
six listed source files differ; package archive and unchanged-file verification
are recorded in the local verification log. The workspace lockfile retains all
existing dependency versions and selects this local patch only for Parley.

Varta's regressions cover maximum Unicode/password field display, source hits,
IME/textarea/placeholder behavior, public byte-limit isolation, exact glyph
counts around 255/256, long-cluster fallback coverage and the existing 65536-glyph
paint boundary with refusal and recovery. Original synthetic fonts and their
regeneration script live in the repository; no third-party font is copied.

Reassess/remove this patch when an upstream dependency update supplies equivalent
wide-offset/count, complete-coverage and bounded/fallible request behavior and
passes those regressions.
Do not replace it with a registry-cache edit or text truncation/chunking.
