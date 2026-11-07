# Varta Taffy patch

Taffy 0.15.1 is copied from its checksum-pinned crates.io release under its
MIT license (`Cargo.toml`). Upstream: https://github.com/DioxusLabs/taffy ;
package: https://crates.io/crates/taffy/0.15.1 . Original archive SHA-256:
`31f7bd2d97198df05186f93a3a3c306845f40e8b3c22bf9279cdd7e5cf5fea7d`.

`VARTA-PATCH.diff` records one change to `src/tree/taffy_tree.rs`.

Upstream's `calc` feature stores a `calc()` length as an opaque handle and asks
the tree to resolve it against the percentage basis of its context
(`LayoutPartialTree::resolve_calc_value`). The high-level `TaffyTree` always
answers zero, so a host that lays out through `TaffyTree` cannot use `calc()`.
The patch adds `TaffyTree::set_calc_resolver`, a plain function the tree calls
with the handle and the basis; without one the tree answers zero as upstream.

Varta's native layout (`varta-view`) lowers a CSS Values 4 math function that
sums lengths with percentages to a `calc()` of `px` and `%` leaves and passes
an index into a per-layout table as the handle, so neither side dereferences a
pointer. Nothing else changes; Taffy never dereferences calc handles.
