#!/usr/bin/env python3
"""Original geometric face for underline/overhang and decoration-order probes."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

family = 'Varta Decoration Fixture'
names = ['.notdef', 'space', 'A', 'g', 'h', 'j', 'k']
font = FontBuilder(1000, isTTF=True)
font.setupGlyphOrder(names)
font.setupCharacterMap({32: 'space', 65: 'A', 103: 'g', 104: 'h', 106: 'j', 107: 'k', 0x4e00: 'g', 0x05d0: 'A', 0x05d1: 'h', 0x05d2: 'g'})
glyphs = {}
for name in names:
    pen = TTGlyphPen(None)
    if name == 'j':
        pen.moveTo((50, 700)); pen.lineTo((350, 700)); pen.lineTo((350, -50))
        pen.qCurveTo((350, -250), (150, -250)); pen.qCurveTo((-50, -250), (-50, -100))
        pen.lineTo((50, -100)); pen.qCurveTo((50, -150), (150, -150))
        pen.qCurveTo((250, -150), (250, -50)); pen.lineTo((50, 0)); pen.closePath()
    elif name != 'space':
        left, right = (50, 650) if name in ('h', 'k') else (50, 350)
        bottom = -250 if name in ('g', 'k') else 0
        pen.moveTo((left, bottom)); pen.lineTo((left, 700))
        pen.lineTo((right, 700)); pen.lineTo((right, bottom)); pen.closePath()
    glyphs[name] = pen.glyph()
font.setupGlyf(glyphs)
font.setupHorizontalMetrics({name: (400, 0 if name == 'space' else 50) for name in names})
font.setupHorizontalHeader(ascent=900, descent=-300)
font.setupNameTable({'familyName': family, 'styleName': 'Regular', 'uniqueFontIdentifier': 'VartaDecorationFixture',
                    'fullName': family, 'psName': 'VartaDecorationFixture'})
font.setupOS2(sTypoAscender=900, sTypoDescender=-300, usWinAscent=900, usWinDescent=300)
font.setupPost(); font.setupMaxp()
font.font.recalcTimestamp = False
font.font['head'].created = font.font['head'].modified = 2082844800
font.save(Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures/decoration.ttf')
