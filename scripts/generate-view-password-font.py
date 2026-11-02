#!/usr/bin/env python3
"""Generate an original font with visibly distinct plaintext and password masks."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

root = Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures'
builder = FontBuilder(1000, isTTF=True)
builder.setupGlyphOrder(['.notdef', 'space', 'A', 'bullet'])
builder.setupCharacterMap({32: 'space', 65: 'A', 0x2022: 'bullet'})
glyphs = {}
for name in ('.notdef', 'space', 'A', 'bullet'):
    pen = TTGlyphPen(None)
    if name != 'space':
        left, bottom, right, top = (125, 250, 275, 400) if name == 'bullet' else (50, 0, 350, 700)
        pen.moveTo((left, bottom)); pen.lineTo((left, top))
        pen.lineTo((right, top)); pen.lineTo((right, bottom)); pen.closePath()
    glyphs[name] = pen.glyph()
builder.setupGlyf(glyphs)
builder.setupHorizontalMetrics({name: (400, 125 if name == 'bullet' else 50 if name != 'space' else 0) for name in glyphs})
builder.setupHorizontalHeader(ascent=800, descent=-200)
builder.setupNameTable({'familyName': 'Varta Password Fixture', 'styleName': 'Regular', 'uniqueFontIdentifier': 'VartaPasswordFixture', 'fullName': 'Varta Password Fixture', 'psName': 'VartaPasswordFixture'})
builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
builder.setupPost(); builder.setupMaxp()
builder.font.recalcTimestamp = False
builder.font['head'].created = builder.font['head'].modified = 2082844800
builder.save(root / 'password.ttf')
