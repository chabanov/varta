#!/usr/bin/env python3
"""Generate original finite and looping AAT insertion-state fixtures."""
from pathlib import Path
import struct
from fontTools.ttLib import TTFont
from fontTools.ttLib.tables.DefaultTable import DefaultTable

root = Path(__file__).resolve().parent.parent / 'crates/varta-view/tests/fixtures'

def words(*values):
    return struct.pack('>' + 'H' * len(values), *values)

def longs(*values):
    return struct.pack('>' + 'I' * len(values), *values)

for kind, family in [
    ('current', 'Varta AAT Current'),
    ('marked', 'Varta AAT Marked'),
    ('loop', 'Varta AAT Loop'),
]:
    font = TTFont(root / 'clusters.ttf', recalcTimestamp=False)
    del font['GSUB']
    for record in font['name'].names:
        text = {1: family, 3: family.replace(' ', ''), 4: family,
                6: family.replace(' ', '')}.get(record.nameID)
        if text is not None:
            record.string = text.encode(record.getEncoding())
    # Extended state table: class 4 is A (glyph 2); other glyphs use default.
    lookup = words(8, 2, 1, 4)
    states = words(0, 0, 0, 0, 1, 0, 0, 0, 0, 2)
    default = words(0, 0, 0xFFFF, 0xFFFF)
    if kind == 'marked':
        # Mark first A; on the second insert 3 at the mark and 2 at current.
        first = words(1, 0x8000, 0xFFFF, 0xFFFF)
        second = words(0, 0x0400 | 0x0800 | 3 | (2 << 5), 1, 0)
    else:
        # Revisit A once without advancing; either insert 3 or loop forever.
        first = words(1, 0x4000, 0xFFFF, 0xFFFF)
        second = (words(1, 0x4000, 0xFFFF, 0xFFFF) if kind == 'loop'
                  else words(0, 0x0800 | (3 << 5), 0, 0xFFFF))
    entries = default + first + second
    # 16-byte state header plus 4-byte insertion-glyph-table offset.
    class_offset = 20
    state_offset = class_offset + len(lookup)
    entry_offset = state_offset + len(states)
    glyph_offset = entry_offset + len(entries)
    body = (longs(5, class_offset, state_offset, entry_offset, glyph_offset)
            + lookup + states + entries + words(5, 6, 7, 0))
    subtable = longs(12 + len(body), 5, 1) + body
    chain = longs(1, 16 + len(subtable), 0, 1) + subtable
    table = DefaultTable('morx')
    table.data = longs(0x00020000, 1) + chain
    font['morx'] = table
    font['head'].created = font['head'].modified = 2082844800
    font.save(root / f'aat-{kind}.ttf')

# Legacy and extended kerning state machines revisit A once without actions.
for kind, family in [('kern', 'Varta AAT Kern'), ('kerx', 'Varta AAT Kerx')]:
    font = TTFont(root / 'clusters.ttf', recalcTimestamp=False)
    del font['GSUB']
    for record in font['name'].names:
        text = {1: family, 3: family.replace(' ', ''), 4: family,
                6: family.replace(' ', '')}.get(record.nameID)
        if text is not None:
            record.string = text.encode(record.getEncoding())
    if kind == 'kern':
        classes = words(2, 1) + bytes([4])
        states = bytes([0, 0, 0, 0, 1, 0, 0, 0, 0, 2])
        class_offset = 10
        state_offset = class_offset + len(classes)
        entry_offset = state_offset + len(states)
        entries = words(state_offset, 0, state_offset + 5, 0x4000, state_offset, 0)
        value_offset = entry_offset + len(entries)
        body = (words(5, class_offset, state_offset, entry_offset, value_offset)
                + classes + states + entries + words(0))
        body += b'\0' * (-len(body) % 4)
        raw = longs(0x00010000, 1) + longs(8 + len(body)) + words(1, 0) + body
    else:
        lookup = words(8, 2, 1, 4)
        states = words(0, 0, 0, 0, 1, 0, 0, 0, 0, 2)
        class_offset = 20
        state_offset = class_offset + len(lookup)
        entry_offset = state_offset + len(states)
        entries = words(0, 0, 0xFFFF, 1, 0x4000, 0xFFFF, 0, 0, 0xFFFF)
        value_offset = entry_offset + len(entries)
        body = (longs(5, class_offset, state_offset, entry_offset, value_offset)
                + lookup + states + entries + words(0))
        raw = words(2, 0) + longs(1, 12 + len(body), 1, 0) + body
    table = DefaultTable(kind)
    table.data = raw
    font[kind] = table
    font['head'].created = font['head'].modified = 2082844800
    font.save(root / f'aat-{kind}.ttf')
