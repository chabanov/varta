#!/usr/bin/env python3
"""Generate original glyph-count fixtures; requires fontTools only to regenerate."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

root = Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures'
names = ['.notdef', 'space', 'A', 'mark', 'single', 'part1', 'part2', 'part3']
for family, filename, advance, cmap, expansion in [
    ('Varta Cluster Fixture', 'clusters.ttf', 400,
     {32: 'space', 65: 'A', 0x035C: 'mark', 0x035D: 'single'}, 3),
    ('Varta Cluster Partial', 'clusters-partial.ttf', 900,
     {32: 'space', 65: 'A', 0x035D: 'single'}, 3),
    ('Varta Expansion Fixture', 'expansion.ttf', 400,
     {32: 'space', 65: 'A', 0x035C: 'mark', 0x035D: 'single'}, 16),
]:
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(names)
    builder.setupCharacterMap(cmap)
    glyphs = {}
    for name in names:
        pen = TTGlyphPen(None)
        if name != 'space':
            pen.moveTo((50, 0))
            pen.lineTo((50, 700))
            pen.lineTo((350, 700))
            pen.lineTo((350, 0))
            pen.closePath()
        glyphs[name] = pen.glyph()
    builder.setupGlyf(glyphs)
    builder.setupHorizontalMetrics({name: (advance if name in ['.notdef', 'space', 'A'] else 0, 50) for name in names})
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    builder.setupNameTable({
        'familyName': family, 'styleName': 'Regular',
        'uniqueFontIdentifier': family.replace(' ', ''),
        'fullName': family, 'psName': family.replace(' ', ''),
    })
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
    builder.setupPost()
    builder.setupMaxp()
    parts = ' '.join(f'part{i % 3 + 1}' for i in range(expansion))
    addOpenTypeFeaturesFromString(builder.font, f'''
    feature ccmp {{ sub mark by {parts}; }} ccmp;
    table GDEF {{ GlyphClassDef [A], , [mark single part1 part2 part3], ; }} GDEF;
    ''')
    builder.font.recalcTimestamp = False
    builder.font['head'].created = builder.font['head'].modified = 2082844800
    builder.save(root / filename)
