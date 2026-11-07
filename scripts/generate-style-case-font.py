#!/usr/bin/env python3
"""Generate the style engine's text-transform fixture (CSS Text 3 §2.1).

`case.ttf` (`Varta Case`) is an original synthetic face with rectangle
outlines whose advances tell every mapping of the Chrome comparison apart:
lowercase and uppercase Latin letters, `ß` and its `SS`, the full-width
forms of `A` and the space, and small and full-size kana. The scene draws
the text transparent, so each box's width shows which characters the
renderer shaped. Requires fontTools only to regenerate; fixed timestamps
make the output reproducible (verified with fontTools 4.60.2).
"""
from pathlib import Path
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

ROOT = Path(__file__).resolve().parent.parent / "crates/varta-style/tests/fixtures"
EPOCH = 2082844800
# (name, code point, advance): every advance is distinct.
GLYPHS = [
    ("space", 0x20, 250),
    ("hyphen", 0x2D, 310),
    ("A", 0x41, 700),
    ("B", 0x42, 720),
    ("S", 0x53, 650),
    ("a", 0x61, 500),
    ("b", 0x62, 520),
    ("s", 0x73, 450),
    ("germandbls", 0xDF, 560),
    ("ideographicspace", 0x3000, 1010),
    ("asmall", 0x3041, 610),
    ("a_hiragana", 0x3042, 930),
    ("A_fullwidth", 0xFF21, 1030),
    ("a_halfwidth_small", 0xFF67, 470),
    ("a_halfwidth", 0xFF71, 530),
]


def rectangle(advance):
    pen = TTGlyphPen(None)
    pen.moveTo((40, 0))
    pen.lineTo((40, 700))
    pen.lineTo((advance - 40, 700))
    pen.lineTo((advance - 40, 0))
    pen.closePath()
    return pen.glyph()


def main():
    names = [".notdef"] + [name for name, _, _ in GLYPHS]
    advances = {".notdef": 600, **{name: advance for name, _, advance in GLYPHS}}
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(names)
    builder.setupCharacterMap({code: name for name, code, _ in GLYPHS})
    builder.setupGlyf({
        name: TTGlyphPen(None).glyph() if name in ("space", "ideographicspace") else rectangle(advances[name])
        for name in names
    })
    builder.setupHorizontalMetrics({name: (advances[name], 40) for name in names})
    builder.setupHorizontalHeader(ascent=800, descent=-200, lineGap=0)
    builder.setupNameTable({
        "familyName": "Varta Case", "styleName": "Regular", "uniqueFontIdentifier": "VartaCaseRegular",
        "fullName": "Varta Case Regular", "psName": "VartaCaseRegular",
    })
    builder.setupOS2(version=4, usWeightClass=400, sTypoAscender=800, sTypoDescender=-200, sTypoLineGap=0,
                     usWinAscent=800, usWinDescent=200, sxHeight=500, sCapHeight=700)
    builder.setupPost()
    builder.setupMaxp()
    builder.font.recalcTimestamp = False
    builder.font["head"].created = builder.font["head"].modified = EPOCH
    builder.font.save(ROOT / "case.ttf")


if __name__ == "__main__":
    main()
