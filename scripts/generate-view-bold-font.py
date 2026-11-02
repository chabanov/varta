#!/usr/bin/env python3
"""Create a deterministic synthetic bold face for Varta's original test font."""
from pathlib import Path
from fontTools.ttLib import TTFont

root = Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures'
font = TTFont(root / 'advance-900.ttf', recalcTimestamp=False)
names = {1: 'Varta Fixture 400', 2: 'Bold', 3: 'VartaFixture400-Bold', 4: 'Varta Fixture 400 Bold', 6: 'VartaFixture400-Bold'}
for record in list(font['name'].names):
    if record.nameID in names:
        font['name'].setName(names[record.nameID], record.nameID, record.platformID, record.platEncID, record.langID)
font['OS/2'].usWeightClass = 700
font['OS/2'].fsSelection = (font['OS/2'].fsSelection | (1 << 5)) & ~(1 << 6)
font['head'].macStyle |= 1
font.save(root / 'bold-900.ttf')
