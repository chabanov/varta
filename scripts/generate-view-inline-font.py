#!/usr/bin/env python3
"""Original ligature/joining/mark fixture; fontTools is regeneration-only."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

family = 'Varta Inline Fixture'
names = ['.notdef', 'space', 'A', 'AA', 'mark', 'alef', 'lam', 'lam.init', 'alef.fina']
b = FontBuilder(1000, isTTF=True)
b.setupGlyphOrder(names)
b.setupCharacterMap({32: 'space', 65: 'A', 0x035d: 'mark', 0x0627: 'alef', 0x0644: 'lam'})
glyphs = {}
for name in names:
    pen = TTGlyphPen(None)
    if name != 'space':
        left, bottom, right, top = (
            (50, 0, 650, 700) if name == 'AA'
            else (100, 750, 300, 850) if name == 'mark'
            else (50, 0, 150, 500) if name.endswith(('init', 'fina'))
            else (50, 0, 350, 700)
        )
        pen.moveTo((left, bottom)); pen.lineTo((left, top))
        pen.lineTo((right, top)); pen.lineTo((right, bottom)); pen.closePath()
    glyphs[name] = pen.glyph()
b.setupGlyf(glyphs)
b.setupHorizontalMetrics({name: (700 if name == 'AA' else 0 if name == 'mark' else 400,
                                   0 if name == 'space' else glyphs[name].xMin) for name in names})
b.setupHorizontalHeader(ascent=900, descent=-100)
b.setupNameTable({'familyName': family, 'styleName': 'Regular', 'uniqueFontIdentifier': 'VartaInlineFixture',
                  'fullName': family, 'psName': 'VartaInlineFixture'})
b.setupOS2(sTypoAscender=900, sTypoDescender=-100, usWinAscent=900, usWinDescent=100)
b.setupPost(); b.setupMaxp()
addOpenTypeFeaturesFromString(b.font, '''
languagesystem DFLT dflt;
languagesystem latn dflt;
languagesystem arab dflt;
feature liga { sub A A by AA; } liga;
feature init { script arab; sub lam by lam.init; } init;
feature fina { script arab; sub alef by alef.fina; } fina;
''')
b.font.recalcTimestamp = False
b.font['head'].created = b.font['head'].modified = 2082844800
b.save(Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures/inline.ttf')
