#!/usr/bin/env python3
"""Generate original tiny, deterministic font fixtures (requires fontTools).
No system or third-party font is copied; glyph outlines and metrics live here.
"""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

root = Path(__file__).resolve().parent.parent / 'crates/varta-project/tests/fixtures/browser-fonts'
root.mkdir(parents=True, exist_ok=True)
for advance in (400, 900):
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(['.notdef', 'space', 'A'])
    builder.setupCharacterMap({32: 'space', 65: 'A'})
    glyphs = {}
    for name in ('.notdef', 'space', 'A'):
        pen = TTGlyphPen(None)
        if name != 'space':
            pen.moveTo((50, 0)); pen.lineTo((50, 700))
            pen.lineTo((advance - 50, 700)); pen.lineTo((advance - 50, 0)); pen.closePath()
        glyphs[name] = pen.glyph()
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics({name: (advance, 50 if name != 'space' else 0) for name in glyphs})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({'familyName': f'Varta Fixture {advance}', 'styleName': 'Regular', 'uniqueFontIdentifier': f'VartaFixture{advance}', 'fullName': f'Varta Fixture {advance}', 'psName': f'VartaFixture{advance}'})
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    builder.setupPost(); builder.setupMaxp()
    builder.font.recalcTimestamp = False
    builder.font['head'].created = builder.font['head'].modified = 2082844800
    builder.save(root / f'advance-{advance}.ttf')
    # FontTools derives the WOFF directory/checksums from the same original face.
    builder.font.flavor = 'woff'
    builder.save(root / f'advance-{advance}.woff')
    builder.font.flavor = 'woff2'
    builder.save(root / f'advance-{advance}.woff2')
    from fontTools.pens.t2CharStringPen import T2CharStringPen
    cff = FontBuilder(1000, isTTF=False)
    cff.setupGlyphOrder(['.notdef', 'space', 'A'])
    cff.setupCharacterMap({32: 'space', 65: 'A'})
    charstrings = {}
    for name in ('.notdef', 'space', 'A'):
        pen = T2CharStringPen(advance, None)
        if name != 'space':
            pen.moveTo((50, 0)); pen.lineTo((50, 700))
            pen.lineTo((advance - 50, 700)); pen.lineTo((advance - 50, 0)); pen.closePath()
        charstrings[name] = pen.getCharString()
    cff.setupCFF(f'VartaCffFixture{advance}', {'FullName': f'Varta CFF Fixture {advance}', 'FamilyName': f'Varta CFF Fixture {advance}', 'Weight': 'Regular'}, charstrings, {})
    cff.setupHorizontalMetrics({name: (advance, 50 if name != 'space' else 0) for name in charstrings})
    cff.setupHorizontalHeader(ascent=800, descent=-200)
    cff.setupNameTable({'familyName': f'Varta CFF Fixture {advance}', 'styleName': 'Regular', 'uniqueFontIdentifier': f'VartaCffFixture{advance}', 'fullName': f'Varta CFF Fixture {advance}', 'psName': f'VartaCffFixture{advance}'})
    cff.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    cff.setupPost(); cff.setupMaxp()
    cff.font.recalcTimestamp = False
    cff.font['head'].created = cff.font['head'].modified = 2082844800
    cff.save(root / f'advance-{advance}.otf')
