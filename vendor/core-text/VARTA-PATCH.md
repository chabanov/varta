# Varta core-text patch

This is `core-text` 21.0.0 from its crates.io release, used under its supplied
MIT OR Apache-2.0 licenses (both files and `COPYRIGHT` are retained).
Upstream: <https://github.com/servo/core-foundation-rs>, release commit
`b10f1efc48343fc5590127ec6d890ffbb8b5bd02`, package path `core-text`.
Original crates.io package SHA-256 (from the preceding lockfile):
`a593227b66cbd4007b2a050dfdd9e1d1318311409c8d600dc82ba1b15ca9c130`.

The normalized manifest, README, licenses and sources were copied from the
release; registry bookkeeping (`.cargo-ok`, `.cargo_vcs_info.json`,
`Cargo.toml.orig`) and the upstream CI and ignore files were omitted.
`VARTA-PATCH.md` and `VARTA-PATCH.diff` are Varta-specific documentation, and
applying the diff to the release `src/` reproduces the vendored tree exactly.

Only `src/font.rs` differs. It declares CoreText's
`CTFontCreateForStringWithLanguage` and adds one safe method,
`CTFont::create_for_string_with_language(&self, string, language)`: the font
CoreText draws the start of `string` with when this font lacks it, exactly the
call Chrome's `FontCache::PlatformFallbackFontForCharacter` makes on macOS. The
`unsafe` block passes only CoreFoundation objects the method owns or borrows
for the call and wraps the returned font under the create rule, like the
crate's other constructors. The workspace forbids `unsafe` code, and
`objc2-core-text` exposes the call only as an `unsafe fn`, so the shell's
platform fallback faces (`varta-shell --system-fonts`) need this wrapper.
CoreText's default cascade list, which the release already wraps, is not
enough: `CTFontCreateForString` also resolves hidden fallback fonts and
language-specific entries the plain list leaves out (for U+2699 the list
names the hidden `.Apple Symbols Fallback`, while CoreText and Chrome draw
Menlo).

The workspace lockfile keeps every existing version and selects this local
patch only for `core-text`. Varta's regressions (`shell::fallback` in
`varta-desktop`) check the faces it names against Chrome 156's
`CSS.getPlatformFontsForNode` on macOS 27, and `native_fallback_probe` with
`scripts/test-native-inline.mjs` compares the resulting geometry and pixels
with Chrome. Remove this patch when a safe upstream wrapper exists.
