#!/usr/bin/env python3
"""Extend Varta's original test-only font with deterministic Cyrillic aliases."""
from pathlib import Path
from fontTools.ttLib import TTFont

root = Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures'
font = TTFont(root / 'advance-400.ttf', recalcTimestamp=False)
for table in font['cmap'].tables:
    if table.isUnicode():
        for codepoint in (0x0021, 0x002c, 0x0410, 0x0430, 0x0456, 0x0457):
            table.cmap[codepoint] = 'A'
font.save(root / 'cyrillic-400.ttf')
