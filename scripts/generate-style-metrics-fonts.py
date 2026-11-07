#!/usr/bin/env python3
"""Generate the style engine's font-metric fixtures (CSS Values 4 §6.1.1).

Original synthetic faces with rectangle outlines whose metrics are chosen to
be told apart: distinct x-heights, cap-heights, `0` and `水` advances and
vertical metrics per face, so `ex`, `cap`, `ch`, `ic` and `lh` show which face
and which table value the engine read. Requires fontTools only to regenerate;
fixed timestamps make the output reproducible (verified with fontTools
4.60.2).

- `metrics.ttf` / `metrics-bold.ttf`: `Varta Metrics` at weights 400 and 700,
  `OS/2` version 4 with an x-height equal to the `x` outline's top (Chrome on
  macOS measures the outline, the specification prefers the metric; equal
  values keep the Chrome oracle exact);
- `metrics-bare.ttf`: `Varta Metrics Bare`, `OS/2` version 1 (no x-height or
  cap-height) mapping only space and `A`: every Values 4 fallback;
- `metrics-variable.ttf`: `Varta Metrics Variable`, a `wght` 100 to 900 face
  whose x-height (`MVAR`), cap-height and `0` advance (`HVAR`) vary.
"""
from pathlib import Path
from fontTools.designspaceLib import AxisDescriptor, DesignSpaceDocument, SourceDescriptor
from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen
from fontTools import varLib

ROOT = Path(__file__).resolve().parent.parent / "crates/varta-style/tests/fixtures"
EPOCH = 2082844800
NAMES = [".notdef", "space", "A", "H", "x", "zero", "water"]
CMAP = {0x20: "space", 0x41: "A", 0x48: "H", 0x78: "x", 0x30: "zero", 0x6C34: "water"}


def rectangle(top, bottom=0):
    pen = TTGlyphPen(None)
    pen.moveTo((50, bottom))
    pen.lineTo((50, top))
    pen.lineTo((300, top))
    pen.lineTo((300, bottom))
    pen.closePath()
    return pen.glyph()


def face(family, style, weight, m, names=NAMES, cmap=None, os2_version=4):
    builder = FontBuilder(1000, isTTF=True)
    builder.setupGlyphOrder(names)
    builder.setupCharacterMap(cmap or {code: name for code, name in CMAP.items() if name in names})
    tops = {".notdef": 700, "A": 700, "H": m["cap"], "x": m["x"], "zero": 690, "water": 800}
    builder.setupGlyf({name: rectangle(tops[name]) if name != "space" else TTGlyphPen(None).glyph() for name in names})
    advances = {".notdef": 500, "space": 250, "A": 600, "H": 640, "x": 480, "zero": m["zero"], "water": m["water"]}
    builder.setupHorizontalMetrics({name: (advances[name], 50) for name in names})
    builder.setupHorizontalHeader(ascent=m["ascent"], descent=-m["descent"], lineGap=m["gap"])
    postscript = (family + style).replace(" ", "")
    builder.setupNameTable({
        "familyName": family, "styleName": style, "uniqueFontIdentifier": postscript,
        "fullName": f"{family} {style}", "psName": postscript,
    })
    os2 = dict(version=os2_version, usWeightClass=weight, sTypoAscender=750, sTypoDescender=-250,
               sTypoLineGap=0, usWinAscent=900, usWinDescent=300)
    if os2_version >= 2:
        os2.update(sxHeight=m["x"], sCapHeight=m["cap"])
    builder.setupOS2(**os2)
    if weight >= 700:
        builder.font["OS/2"].fsSelection = 1 << 5
        builder.font["head"].macStyle = 1
    builder.setupPost()
    builder.setupMaxp()
    builder.font.recalcTimestamp = False
    builder.font["head"].created = builder.font["head"].modified = EPOCH
    return builder.font


REGULAR = dict(ascent=830, descent=170, gap=90, x=520, cap=700, zero=555, water=970)
BOLD = dict(ascent=850, descent=190, gap=60, x=540, cap=720, zero=610, water=990)
LIGHT = dict(REGULAR, x=480, cap=680, zero=500)
HEAVY = dict(REGULAR, x=560, cap=740, zero=700)

face("Varta Metrics", "Regular", 400, REGULAR).save(ROOT / "metrics.ttf")
face("Varta Metrics", "Bold", 700, BOLD).save(ROOT / "metrics-bold.ttf")
face("Varta Metrics Bare", "Regular", 400, dict(REGULAR, ascent=900, descent=300, gap=0),
     names=[".notdef", "space", "A"], os2_version=1).save(ROOT / "metrics-bare.ttf")

# The variable face: two masters interpolated by fontTools' varLib.
masters = []
for name, weight, metrics in (("Light", 100, LIGHT), ("Heavy", 900, HEAVY)):
    master = face("Varta Metrics Variable", name, 400, metrics)
    path = ROOT / f"variable-{name}.ttf"
    master.save(path)
    masters.append((path, weight))
document = DesignSpaceDocument()
axis = AxisDescriptor()
axis.tag, axis.name, axis.minimum, axis.default, axis.maximum = "wght", "Weight", 100, 100, 900
document.addAxis(axis)
for path, weight in masters:
    source = SourceDescriptor()
    source.path = str(path)
    source.location = {"Weight": weight}
    document.addSource(source)
variable, _, _ = varLib.build(document)
for path, _ in masters:
    path.unlink()
variable["OS/2"].usWeightClass = 400
variable.recalcTimestamp = False
variable["head"].created = variable["head"].modified = EPOCH
variable.save(ROOT / "metrics-variable.ttf")
