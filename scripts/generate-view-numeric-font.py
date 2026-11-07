#!/usr/bin/env python3
"""Original numeric feature fixture, using fontTools only for regeneration."""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools.feaLib.builder import addOpenTypeFeaturesFromString

family = "Varta Numeric Fixture"
names = [".notdef", "space", "zero", "one", "zero.tab", "one.tab"]
b = FontBuilder(1000, isTTF=True)
b.setupGlyphOrder(names)
b.setupCharacterMap({32: "space", 48: "zero", 49: "one"})
widths = {".notdef": 500, "space": 200, "zero": 400, "one": 200,
          "zero.tab": 600, "one.tab": 600}
glyphs = {}
for name in names:
    pen = TTGlyphPen(None)
    if name != "space":
        pen.moveTo((20, 0))
        pen.lineTo((20, 700))
        pen.lineTo((widths[name]-20, 700))
        pen.lineTo((widths[name]-20, 0))
        pen.closePath()
    glyphs[name] = pen.glyph()
b.setupGlyf(glyphs)
b.setupHorizontalMetrics({name: (widths[name], 0) for name in names})
b.setupHorizontalHeader(ascent=800, descent=-200)
b.setupNameTable({"familyName": family, "styleName": "Regular",
                  "uniqueFontIdentifier": "VartaNumericFixture", "fullName": family,
                  "psName": "VartaNumericFixture"})
b.setupOS2(sTypoAscender=800, sTypoDescender=-200, usWinAscent=800, usWinDescent=200)
b.setupPost()
b.setupMaxp()
addOpenTypeFeaturesFromString(b.font, """
languagesystem DFLT dflt;
feature tnum { sub zero by zero.tab; sub one by one.tab; } tnum;
""")
b.font.recalcTimestamp = False
b.font['head'].created = b.font['head'].modified = 2082844800
b.save(Path(__file__).resolve().parent.parent / "crates/varta-view/tests/fixtures/numeric.ttf")
