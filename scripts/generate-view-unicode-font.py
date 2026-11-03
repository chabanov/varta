#!/usr/bin/env python3
"""Generate original Unicode paragraph glyphs; fontTools is regeneration-only."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

root = Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures'
family = 'Varta Unicode Fixture'
names = ['.notdef', 'space', 'bar', 'square', 'mark']
builder = FontBuilder(1000, isTTF=True)
builder.setupGlyphOrder(names)
builder.setupCharacterMap({
    0x0020: 'space', 0x0041: 'bar', 0x0042: 'square',
    0x00a0: 'space', 0x00e9: 'bar', 0x035d: 'mark',
    0x05d0: 'bar', 0x05d1: 'square',
    0x0627: 'bar', 0x0644: 'square',
    0x2191: 'bar', 0x25c7: 'square', 0x4e2d: 'bar', 0x1f642: 'square',
})
glyphs = {}
for name in names:
    pen = TTGlyphPen(None)
    if name != 'space':
        left, bottom, right, top = (
            (125, 250, 275, 400) if name == 'square'
            else (100, 750, 300, 850) if name == 'mark'
            else (50, 0, 350, 700)
        )
        pen.moveTo((left, bottom))
        pen.lineTo((left, top))
        pen.lineTo((right, top))
        pen.lineTo((right, bottom))
        pen.closePath()
    glyphs[name] = pen.glyph()
builder.setupGlyf(glyphs)
builder.setupHorizontalMetrics({
    name: (0 if name == 'mark' else 400, 0 if name == 'space' else glyphs[name].xMin)
    for name in names
})
builder.setupHorizontalHeader(ascent=900, descent=-100)
builder.setupNameTable({
    'familyName': family, 'styleName': 'Regular',
    'uniqueFontIdentifier': 'VartaUnicodeFixture',
    'fullName': family, 'psName': 'VartaUnicodeFixture',
})
builder.setupOS2(sTypoAscender=900, sTypoDescender=-100, usWinAscent=900, usWinDescent=100)
builder.setupPost()
builder.setupMaxp()
builder.font.recalcTimestamp = False
builder.font['head'].created = builder.font['head'].modified = 2082844800
builder.save(root / 'unicode.ttf')
