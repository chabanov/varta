#!/usr/bin/env python3
"""Generate original regular/bold structure glyphs; fontTools is regeneration-only.

The faces cover list markers (digits, period, bullet, white bullet, small
square), the ellipsis, Latin bars and Hebrew letters for bidirectional
alignment. Bold glyphs have distinct advances and outlines, so face selection
is observable. An optional argument names another output directory.
"""
import sys
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

root = Path(sys.argv[1]) if len(sys.argv) > 1 else (
    Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures')
family = 'Varta Structure Fixture'
names = ['.notdef', 'space', 'bar', 'square', 'digit', 'period', 'bullet',
         'whitebullet', 'smallsquare', 'ellipsis', 'alef', 'bet']
cmap = {
    0x0020: 'space', 0x0041: 'bar', 0x0042: 'square', 0x002e: 'period',
    0x2022: 'bullet', 0x25e6: 'whitebullet', 0x25aa: 'smallsquare',
    0x2026: 'ellipsis', 0x05d0: 'alef', 0x05d1: 'bet',
}
cmap.update({0x0030 + digit: 'digit' for digit in range(10)})


def rect(pen, left, bottom, right, top):
    pen.moveTo((left, bottom))
    pen.lineTo((left, top))
    pen.lineTo((right, top))
    pen.lineTo((right, bottom))
    pen.closePath()


def build(bold):
    widen = 100 if bold else 0
    shapes = {
        'bar': ([(50, 0, 350 + widen, 700)], 400 + widen),
        'square': ([(100, 200, 300 + widen, 400)], 400 + widen),
        'digit': ([(50, 100, 250 + widen, 600), (50, 0, 400 + widen, 100)], 450 + widen),
        'period': ([(50, 0, 150 + widen // 2, 100)], 200 + widen // 2),
        'bullet': ([(50, 250, 250, 450)], 300),
        'whitebullet': ([(50, 250, 250, 300), (50, 400, 250, 450)], 300),
        'smallsquare': ([(50, 300, 200, 450)], 300),
        'ellipsis': ([(50, 0, 150, 100), (250, 0, 350, 100), (450, 0, 550, 100)], 600),
        'alef': ([(50, 0, 350 + widen, 500)], 400 + widen),
        'bet': ([(50, 300, 350 + widen, 700)], 400 + widen),
        'space': ([], 300),
        '.notdef': ([(50, 0, 350, 700)], 400),
    }
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
    builder.setupHorizontalHeader(ascent=800, descent=-200)
    style = 'Bold' if bold else 'Regular'
    ident = 'VartaStructureFixture-' + style
    builder.setupNameTable({
        'familyName': family, 'styleName': style, 'uniqueFontIdentifier': ident,
        'fullName': family + ' ' + style, 'psName': ident,
    })
    builder.setupOS2(sTypoAscender=800, sTypoDescender=-200, sTypoLineGap=0,
                     usWinAscent=800, usWinDescent=200,
                     usWeightClass=700 if bold else 400,
                     fsSelection=(1 << 5) if bold else (1 << 6))
    if bold:
        builder.font['head'].macStyle = 1
    builder.setupPost()
    builder.setupMaxp()
    builder.font.recalcTimestamp = False
    builder.font['head'].created = builder.font['head'].modified = 2082844800
    builder.save(root / ('structure-700.ttf' if bold else 'structure-400.ttf'))


build(False)
build(True)
