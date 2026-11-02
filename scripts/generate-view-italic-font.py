#!/usr/bin/env python3
"""Create a deterministic synthetic italic face for Varta's original test font."""
from pathlib import Path
from fontTools.ttLib import TTFont

root = Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures'
font = TTFont(root / 'advance-900.ttf', recalcTimestamp=False)
names = {1: 'Varta Fixture 400', 2: 'Italic', 3: 'VartaFixture400-Italic', 4: 'Varta Fixture 400 Italic', 6: 'VartaFixture400-Italic'}
for record in list(font['name'].names):
    if record.nameID in names:
        font['name'].setName(names[record.nameID], record.nameID, record.platformID, record.platEncID, record.langID)
font['OS/2'].usWeightClass = 400
font['OS/2'].fsSelection = (font['OS/2'].fsSelection | 1) & ~(1 << 6)
font['head'].macStyle |= 2
font['post'].italicAngle = -12
font.save(root / 'italic-900.ttf')
