#!/usr/bin/env python3
"""Generate the original form-control fixture face; fontTools is regeneration-only.

Native and Chrome size an unsized text field from the primary font's `x`
glyph and ascent, so this face maps `x` (advance 500, distinct from the
800-unit ascent) besides Latin bars, digits, period, hyphen and the bullet
password mask. OS/2 carries an explicit x-height. Glyph outlines are original
rectangles. An optional argument names another output directory.
"""
import sys
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

root = Path(sys.argv[1]) if len(sys.argv) > 1 else (
    Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures')
family = 'Varta Controls Fixture'
names = ['.notdef', 'space', 'bar', 'square', 'x', 'digit', 'period', 'hyphen', 'bullet']
cmap = {
    0x0020: 'space', 0x0041: 'bar', 0x0042: 'square', 0x0078: 'x',
    0x002e: 'period', 0x002d: 'hyphen', 0x2022: 'bullet',
}
cmap.update({0x0030 + digit: 'digit' for digit in range(10)})
ASCENT, DESCENT, X_HEIGHT = 800, 200, 500
shapes = {
    '.notdef': ([(50, 0, 350, 700)], 400),
    'space': ([], 300),
    'bar': ([(50, 0, 350, 700)], 400),
    'square': ([(100, 200, 300, 400)], 400),
    'x': ([(50, 0, 450, X_HEIGHT)], 500),
    'digit': ([(50, 100, 250, 600), (50, 0, 400, 100)], 450),
    'period': ([(50, 0, 150, 100)], 200),
    'hyphen': ([(50, 250, 250, 350)], 300),
    'bullet': ([(50, 250, 250, 450)], 300),
}


def rect(pen, left, bottom, right, top):
    pen.moveTo((left, bottom))
    pen.lineTo((left, top))
    pen.lineTo((right, top))
    pen.lineTo((right, bottom))
    pen.closePath()


builder = FontBuilder(1000, isTTF=True)
builder.setupGlyphOrder(names)
builder.setupCharacterMap(cmap)
glyphs = {}
for name in names:
    pen = TTGlyphPen(None)
    for box in shapes[name][0]:
        rect(pen, *box)
    glyphs[name] = pen.glyph()
builder.setupGlyf(glyphs)
builder.setupHorizontalMetrics({
    name: (shapes[name][1], getattr(glyphs[name], 'xMin', 0)) for name in names
})
builder.setupHorizontalHeader(ascent=ASCENT, descent=-DESCENT)
builder.setupNameTable({
    'familyName': family, 'styleName': 'Regular',
    'uniqueFontIdentifier': 'VartaControlsFixture-Regular',
    'fullName': family + ' Regular', 'psName': 'VartaControlsFixture-Regular',
})
builder.setupOS2(sTypoAscender=ASCENT, sTypoDescender=-DESCENT, sTypoLineGap=0,
                 usWinAscent=ASCENT, usWinDescent=DESCENT, usWeightClass=400,
                 sxHeight=X_HEIGHT, sCapHeight=700, version=4, fsSelection=1 << 6)
builder.setupPost()
builder.setupMaxp()
builder.font.recalcTimestamp = False
builder.font['head'].created = builder.font['head'].modified = 2082844800
builder.save(root / 'controls.ttf')
