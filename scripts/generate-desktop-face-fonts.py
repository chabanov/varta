#!/usr/bin/env python3
"""Original variable and collection faces for the desktop CoreText face tests.

Writes `crates/varta-desktop/tests/fixtures/fonts/variable.ttf` (one `wght`
axis with a nonlinear `avar` map and `gvar` outline deltas),
`variable-names.ttf` (the same face whose Mac and Windows axis-name records
differ, as CoreGraphics and a Windows-first reader would name the axis),
`variable-axes.ttf` (independent `wght` and `wdth` axes, each moving a
different half of the outline) and `collection.ttc` (two members with
different family names and outlines).
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
MAC_PLATFORM = 1
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


def split_deltas(glyf, name, right, left):
    """Per-point x deltas: `right` for the right half, `left` for the left."""
    coordinates, _, _ = glyf[name].getCoordinates(glyf)
    middle = (min(x for x, _ in coordinates) + max(x for x, _ in coordinates)) / 2
    # Phantom points (advance) do not vary.
    return [(right if x > middle else left, 0) for x, _ in coordinates] + [(0, 0)] * 4


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
        # Heavier widens the right half; lighter narrows it.
        variations[name] = [
            TupleVariation({'wght': (0.0, 1.0, 1.0)}, split_deltas(glyf, name, 160, 0)),
            TupleVariation({'wght': (-1.0, -1.0, 0.0)}, split_deltas(glyf, name, -90, 0)),
        ]
    b.setupGvar(variations)
    return b.font


def variable_names():
    """`variable.ttf` with its Mac axis-name record renamed; Windows keeps 'Weight'."""
    font = variable()
    axis = font['fvar'].axes[0].axisNameID
    for record in font['name'].names:
        if record.nameID == axis and record.platformID == MAC_PLATFORM:
            record.string = 'Gewicht'
    return font


def variable_axes():
    glyphs = {'A': [STEM, BOWL], 'B': [DIAMOND]}
    b = builder('Varta Variable Axes Fixture', 'VartaVariableAxesFixture', glyphs)
    b.setupFvar(axes=[('wght', 100, 400, 900, 'Weight'), ('wdth', 50, 100, 200, 'Width')],
                instances=[])
    glyf = b.font['glyf']
    variations = {}
    for name in glyphs:
        # Weight moves the right half and width the left half, so an instance
        # that drops or swaps either axis draws different ink.
        variations[name] = [
            TupleVariation({'wght': (0.0, 1.0, 1.0)}, split_deltas(glyf, name, 160, 0)),
            TupleVariation({'wdth': (0.0, 1.0, 1.0)}, split_deltas(glyf, name, 0, -120)),
        ]
    b.setupGvar(variations)
    return b.font


def reloaded(font):
    data = BytesIO()
    font.save(data)
    data.seek(0)
    # Saving the collection must not stamp the current time into `head`.
    return TTFont(data, recalcTimestamp=False)


def collection():
    zero = builder('Varta Collection Zero', 'VartaCollectionZero', {'A': [STEM]})
    one = builder('Varta Collection One', 'VartaCollectionOne', {'A': [DIAMOND], 'B': [STEM, BOWL]})
    result = TTCollection()
    result.fonts = [reloaded(zero.font), reloaded(one.font)]
    return result


OUT.mkdir(parents=True, exist_ok=True)
variable().save(OUT / 'variable.ttf')
variable_names().save(OUT / 'variable-names.ttf')
variable_axes().save(OUT / 'variable-axes.ttf')
collection().save(OUT / 'collection.ttc', shareTables=True)
