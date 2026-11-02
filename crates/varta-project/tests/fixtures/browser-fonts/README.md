These original minimal fonts belong to this repository and use its license. No
system font or third-party outline is included. The only drawn shape is a rectangle;
`A` has a known advance of 400 or 900 units in a 1000-unit em. Space is empty.

Regenerate with `python3 scripts/generate-browser-font-fixtures.py` (fontTools and
Brotli support required only for regeneration). Fixed timestamps and glyph order
make the files reproducible. Tests consume the checked-in bytes and need no Python
or font installation. They compare real browser decoding and text metrics, including
two concurrent sessions using different bytes under the same logical family.
