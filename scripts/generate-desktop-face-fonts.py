#!/usr/bin/env python3
"""Original variable and collection faces for the desktop CoreText face tests.

Writes `crates/varta-desktop/tests/fixtures/fonts/variable.ttf` (one `wght`
axis with a nonlinear `avar` map and `gvar` outline deltas) and
`collection.ttc` (two members with different family names and outlines).
All outlines are original; fixed timestamps keep the output reproducible.
fontTools is needed only for regeneration.
"""
from io import BytesIO
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.ttLib import TTFont, newTable
from fontTools.ttLib.tables.TupleVariation import TupleVariation
from fontTools.ttLib.ttCollection import TTCollection

OUT = Path(__file__).resolve().parent.parent / 'crates/varta-desktop/tests/fixtures/fonts'
EPOCH = 2082844800  # 1904-01-01, the OpenType `head` epoch.
UPM = 1000
ADVANCE = 600


def contour(pen, points):
    """Points are (x, y, on_curve); a run of off-curve points is quadratic."""
    pen.moveTo(points[0][:2])
    pending = []
    for x, y, on in points[1:]:
        if on:
            if pending:
                pen.qCurveTo(*pending, (x, y))
                pending = []
            else:
                pen.lineTo((x, y))
        else:
            pending.append((x, y))
    if pending:
        pen.qCurveTo(*pending, points[0][:2])
    pen.closePath()


# A slanted stem with a curved bowl: diagonal and curved edges exercise
# antialiasing, unlike the rectangle fixtures.
STEM = [(80, 0, True), (230, 0, True), (420, 700, True), (270, 700, True)]
BOWL = [(260, 120, True), (520, 140, False), (540, 360, True), (420, 300, False), (300, 260, True)]
DIAMOND = [(300, -100, True), (560, 300, True), (300, 760, False), (40, 300, True)]


def builder(family, ps_name, glyphs):
    names = ['.notdef', 'space'] + list(glyphs)
    b = FontBuilder(UPM, isTTF=True)
    b.setupGlyphOrder(names)
    b.setupCharacterMap({32: 'space', **{ord(name): name for name in glyphs}})
    outlines = {}
    for name in names:
        pen = TTGlyphPen(None)
        for points in glyphs.get(name, []):
            contour(pen, points)
        outlines[name] = pen.glyph()
    b.setupGlyf(outlines)
    b.setupHorizontalMetrics({name: (ADVANCE, getattr(outlines[name], 'xMin', 0)) for name in names})
    b.setupHorizontalHeader(ascent=900, descent=-200)
    b.setupNameTable({'familyName': family, 'styleName': 'Regular', 'uniqueFontIdentifier': ps_name,
                      'fullName': family, 'psName': ps_name})
    b.setupOS2(sTypoAscender=900, sTypoDescender=-200, usWinAscent=900, usWinDescent=200,
               usWeightClass=400)
    b.setupPost()
    b.setupMaxp()
    b.font.recalcTimestamp = False
    b.font['head'].created = b.font['head'].modified = EPOCH
    return b


def variable():
    glyphs = {'A': [STEM, BOWL], 'B': [DIAMOND]}
    b = builder('Varta Variable Fixture', 'VartaVariableFixture', glyphs)
    b.setupFvar(axes=[('wght', 100, 400, 900, 'Weight')], instances=[])
    avar = newTable('avar')
    # Nonlinear in both directions, so a normalized coordinate cannot be
    # mistaken for a linearly scaled user value.
    avar.segments = {'wght': {-1.0: -1.0, -0.5: -0.25, 0.0: 0.0, 0.5: 0.75, 1.0: 1.0}}
    b.font['avar'] = avar
    variations = {}
    glyf = b.font['glyf']
    for name in glyphs:
        coordinates, _, _ = glyf[name].getCoordinates(glyf)
        middle = (min(x for x, _ in coordinates) + max(x for x, _ in coordinates)) / 2
        # Heavier widens the right half; lighter narrows it. Phantom points
        # (advance) do not vary.
        heavy = [(160 if x > middle else 0, 0) for x, _ in coordinates] + [(0, 0)] * 4
        light = [(-90 if x > middle else 0, 0) for x, _ in coordinates] + [(0, 0)] * 4
        variations[name] = [TupleVariation({'wght': (0.0, 1.0, 1.0)}, heavy),
                            TupleVariation({'wght': (-1.0, -1.0, 0.0)}, light)]
    b.setupGvar(variations)
    return b.font


def reloaded(font):
    data = BytesIO()
    font.save(data)
    data.seek(0)
    return TTFont(data)


def collection():
    zero = builder('Varta Collection Zero', 'VartaCollectionZero', {'A': [STEM]})
    one = builder('Varta Collection One', 'VartaCollectionOne', {'A': [DIAMOND], 'B': [STEM, BOWL]})
    result = TTCollection()
    result.fonts = [reloaded(zero.font), reloaded(one.font)]
    return result


OUT.mkdir(parents=True, exist_ok=True)
variable().save(OUT / 'variable.ttf')
collection().save(OUT / 'collection.ttc', shareTables=True)
