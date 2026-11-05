# Varta AccessKit consumer patch

This is `accesskit_consumer` 0.39.2 from its crates.io release, under the
supplied Apache-2.0 OR MIT licenses. Original release archive SHA-256 (the
`Cargo.lock` checksum it replaces):
`f3da8f27636015191b2264ee1df681503170a632cafd832fdc1edfdd4e3a2fd3`.
Upstream: <https://github.com/AccessKit/accesskit>, release commit
`f3026222559c7f5f46d8f25cb7404de68eb149f0` (the same commit as the vendored
`accesskit_macos`), package path `accesskit_consumer`. The release archive
omits the repository-root license files; both are included here verbatim from
that commit, as copied for `vendor/accesskit_macos`. The normalized manifest,
README and every source file except `src/node.rs` are copied unchanged. Registry
bookkeeping (`.cargo_vcs_info.json`, `Cargo.toml.orig`), the independent
lockfile and `CHANGELOG.md` are omitted. `VARTA-PATCH.diff` records all source
differences.

## Clip-aware hit test (wave 3)

Every AccessKit platform adapter Varta links answers a platform hit test with
this crate's `NodeRef::node_at_point`, its only public hit-test entry point:
`accesskit_macos` (`accessibilityHitTest:`), and through `accesskit_winit`
also `accesskit_unix` (AT-SPI, the Linux and Pi 5 path) and
`accesskit_windows` (UIA). Upstream's search returns the deepest filtered node
whose bounds contain the point and ignores `clips_children`. Varta publishes
the unclipped layout boxes of scrolled-out and clipped-away content so that
assistive clients can see where it is, so with upstream's search a point over
visible content (a heading above a scrolled pane) returned the invisible node
whose box lay there, on every platform. (The AT-SPI and UIA adapter sources
are not built on macOS and are absent from this machine's registry cache; their
`node_at_point` call sites are those of the upstream release and were not
re-read offline.)

The patched private `hit_test` is upstream's (same filter, same last-to-first
order, same transforms) except that a node with `clips_children` and bounds has
its descendants searched only where the point lies inside those bounds. A
clipping node without bounds has no area to clip to and is searched as before.
`Rect::contains` is half-open, as upstream uses it for the node's own test.
`node_at_point` and `text_position_at_point` share this search; Varta's field
text runs are already clipped inside their field, which Varta marks
`clips_children`, so text positions are unchanged. No `unsafe` code, public API
or dependency changes.

Varta sets `clips_children` only on a container whose presented descendants all
lie inside its bounds, and publishes a hidden box only where the frame and its
clipping ancestors keep this search away from it
(`FrameMetadata::accessibility_geometry`). AccessKit's filter is unchanged: for
a clipping node that is a filtered parent (a scroll area), `common_filter`
omits children beyond the nearest one past each edge until scrolling brings
them next to the area.

Desktop regressions (`crates/varta-desktop/src/accessibility/geometry_tests.rs`)
search real Varta publications through this crate's public `node_at_point`
under `common_filter`: a pane scrolled over the heading before it and a box
moved out of its clip box over earlier content return the heading, points in
the pane return the visible items, and in each case the hidden box contains the
point and passes the filter, so the unpatched search returns it (checked by
temporarily restoring the release `src/node.rs`: both regressions then fail with
the hidden node). These are consumer-level checks run on macOS; no AT-SPI or
UIA client was driven.

Verification status: the reach of this patch to AT-SPI and UIA follows from
the single `accesskit_consumer` lock entry that `accesskit_atspi_common`,
`accesskit_windows` and `accesskit_macos` share, not from a build. Varta's
desktop crate was not compiled for a Linux or Windows target with this patch
(the Linux dependency sources are not in the offline cache and no Windows
target is installed); only this crate alone was checked for
`aarch64-unknown-linux-musl` and `x86_64-unknown-linux-musl`. Linux and Windows
success needs a CI or Pi 5 build before it is claimed.

Reassess/remove the patch when an upstream release honours `clips_children` in
its hit test and passes those regressions. Do not edit the registry cache.
