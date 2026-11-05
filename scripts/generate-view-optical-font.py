#!/usr/bin/env python3
"""Original variable face with an optical-size axis for the native text tests.

Writes `crates/varta-view/tests/fixtures/optical.ttf`: family
`Varta Optical Fixture`, one rectangle glyph A mapped from U+0041, and an
`opsz` axis (min 8, default 12, max 72) whose `gvar` phantom-point deltas move
A's advance from 300 units at opsz 8 through 600 at the default to 1000 at
opsz 72, linearly within each side. Text measured at a CSS size therefore
shows which optical size the shaper chose. Outlines are original and fixed
timestamps keep the output reproducible; fontTools is needed only for
regeneration.
"""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib.tables.TupleVariation import TupleVariation

OUT = Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures/optical.ttf'
EPOCH = 2082844800  # 1904-01-01, the OpenType `head` epoch.
UPM = 1000
FAMILY = 'Varta Optical Fixture'
PS_NAME = 'VartaOpticalFixture'
ADVANCE = 600
SMALLEST = 300
LARGEST = 1000
AXIS = ('opsz', 8, 12, 72, 'Optical size')


def glyph(width):
    pen = TTGlyphPen(None)
    if width:
        pen.moveTo((50, 0))
        pen.lineTo((50, 700))
        pen.lineTo((width, 700))
        pen.lineTo((width, 0))
        pen.closePath()
    return pen.glyph()


names = ['.notdef', 'space', 'A']
b = FontBuilder(UPM, isTTF=True)
b.setupGlyphOrder(names)
b.setupCharacterMap({32: 'space', ord('A'): 'A'})
outlines = {'.notdef': glyph(0), 'space': glyph(0), 'A': glyph(250)}
b.setupGlyf(outlines)
b.setupHorizontalMetrics({name: (ADVANCE, getattr(outlines[name], 'xMin', 0)) for name in names})
b.setupHorizontalHeader(ascent=900, descent=-200)
b.setupNameTable({'familyName': FAMILY, 'styleName': 'Regular', 'uniqueFontIdentifier': PS_NAME,
                  'fullName': FAMILY, 'psName': PS_NAME})
b.setupOS2(sTypoAscender=900, sTypoDescender=-200, usWinAscent=900, usWinDescent=200,
           usWeightClass=400)
b.setupPost()
b.setupMaxp()
b.setupFvar(axes=[AXIS], instances=[])
points = len(b.font['glyf']['A'].getCoordinates(b.font['glyf'])[0])
# Phantom points follow the outline: left side bearing, advance, top, bottom.
advance = lambda delta: [(0, 0)] * points + [(0, 0), (delta, 0), (0, 0), (0, 0)]
b.setupGvar({'A': [
    TupleVariation({'opsz': (0.0, 1.0, 1.0)}, advance(LARGEST - ADVANCE)),
    TupleVariation({'opsz': (-1.0, -1.0, 0.0)}, advance(SMALLEST - ADVANCE)),
]})
b.font.recalcTimestamp = False
b.font['head'].created = b.font['head'].modified = EPOCH
b.font.save(OUT)
