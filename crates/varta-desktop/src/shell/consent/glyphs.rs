//! The prompt layer's own bitmap font: printable ASCII in 5x9 cells (a
//! seven-row cap height and two descender rows).
//!
//! The prompt never draws with the artifact's fonts (a package font could
//! map the glyphs of `Deny` to shapes that read `Allow`) or with any other
//! font the guest can influence. Every character outside printable ASCII
//! is shown as its Unicode escape (`\u{...}`, [`visible`]), so a lookalike
//! letter or an invisible character in a purpose or resource stays visible.
//! A backslash is shown doubled, so text that itself reads `\u{430}` can
//! never pass for the escape of a character. Lines are broken only between
//! the [`units`] of shown text, never inside an escape.

/// Columns and rows of one glyph cell, and the rows above the baseline.
pub(super) const COLUMNS: usize = 5;
pub(super) const ROWS: usize = 9;
pub(super) const CAP_ROWS: usize = 7;
/// The first and last character the table draws.
const FIRST: char = ' ';
const LAST: char = '~';
/// The number of printable ASCII characters.
const GLYPH_COUNT: usize = LAST as usize - FIRST as usize + 1;
/// The cell character of a lit and an unlit font pixel.
const LIT: u8 = b'#';
const UNLIT: u8 = b'.';

/// One row as a bit set: the leftmost column is the highest of the
/// [`COLUMNS`] bits. A malformed row is a compile-time error.
const fn row(art: &str) -> u8 {
    let bytes = art.as_bytes();
    assert!(bytes.len() == COLUMNS, "a glyph row has five cells");
    let mut bits = 0u8;
    let mut index = 0;
    while index < COLUMNS {
        bits <<= 1;
        match bytes[index] {
            LIT => bits |= 1,
            UNLIT => {}
            _ => panic!("a glyph cell is '#' or '.'"),
        }
        index += 1;
    }
    bits
}

const fn table(art: [[&str; ROWS]; GLYPH_COUNT]) -> [[u8; ROWS]; GLYPH_COUNT] {
    let mut glyphs = [[0u8; ROWS]; GLYPH_COUNT];
    let mut glyph = 0;
    while glyph < GLYPH_COUNT {
        let mut line = 0;
        while line < ROWS {
            glyphs[glyph][line] = row(art[glyph][line]);
            line += 1;
        }
        glyph += 1;
    }
    glyphs
}

/// The glyphs from [`FIRST`] to [`LAST`], in code point order, one per
/// line: rows top to bottom, `#` lit; only `g`, `j`, `p`, `q` and `y`
/// reach below the baseline.
#[rustfmt::skip]
const GLYPHS: [[u8; ROWS]; GLYPH_COUNT] = table([
    [".....", ".....", ".....", ".....", ".....", ".....", ".....", ".....", "....."], // space
    ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#..", ".....", "....."], // !
    [".#.#.", ".#.#.", ".#.#.", ".....", ".....", ".....", ".....", ".....", "....."], // "
    [".#.#.", ".#.#.", "#####", ".#.#.", "#####", ".#.#.", ".#.#.", ".....", "....."], // #
    ["..#..", ".####", "#.#..", ".###.", "..#.#", "####.", "..#..", ".....", "....."], // $
    ["##...", "##..#", "...#.", "..#..", ".#...", "#..##", "...##", ".....", "....."], // %
    [".##..", "#..#.", "#.#..", ".#...", "#.#.#", "#..#.", ".##.#", ".....", "....."], // &
    ["..#..", "..#..", ".#...", ".....", ".....", ".....", ".....", ".....", "....."], // '
    ["...#.", "..#..", ".#...", ".#...", ".#...", "..#..", "...#.", ".....", "....."], // (
    [".#...", "..#..", "...#.", "...#.", "...#.", "..#..", ".#...", ".....", "....."], // )
    [".....", "..#..", "#.#.#", ".###.", "#.#.#", "..#..", ".....", ".....", "....."], // *
    [".....", "..#..", "..#..", "#####", "..#..", "..#..", ".....", ".....", "....."], // +
    [".....", ".....", ".....", ".....", ".##..", "..#..", ".#...", ".....", "....."], // ,
    [".....", ".....", ".....", "#####", ".....", ".....", ".....", ".....", "....."], // -
    [".....", ".....", ".....", ".....", ".....", ".##..", ".##..", ".....", "....."], // .
    [".....", "....#", "...#.", "..#..", ".#...", "#....", ".....", ".....", "....."], // /
    [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.", ".....", "....."], // 0
    ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."], // 1
    [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####", ".....", "....."], // 2
    ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###.", ".....", "....."], // 3
    ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.", ".....", "....."], // 4
    ["#####", "#....", "####.", "....#", "....#", "#...#", ".###.", ".....", "....."], // 5
    ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###.", ".....", "....."], // 6
    ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...", ".....", "....."], // 7
    [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.", ".....", "....."], // 8
    [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##..", ".....", "....."], // 9
    [".....", ".##..", ".##..", ".....", ".##..", ".##..", ".....", ".....", "....."], // :
    [".....", ".##..", ".##..", ".....", ".##..", "..#..", ".#...", ".....", "....."], // ;
    ["...#.", "..#..", ".#...", "#....", ".#...", "..#..", "...#.", ".....", "....."], // <
    [".....", ".....", "#####", ".....", "#####", ".....", ".....", ".....", "....."], // =
    [".#...", "..#..", "...#.", "....#", "...#.", "..#..", ".#...", ".....", "....."], // >
    [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#..", ".....", "....."], // ?
    [".###.", "#...#", "....#", ".##.#", "#.#.#", "#.#.#", ".###.", ".....", "....."], // @
    [".###.", "#...#", "#...#", "#...#", "#####", "#...#", "#...#", ".....", "....."], // A
    ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.", ".....", "....."], // B
    [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###.", ".....", "....."], // C
    ["###..", "#..#.", "#...#", "#...#", "#...#", "#..#.", "###..", ".....", "....."], // D
    ["#####", "#....", "#....", "####.", "#....", "#....", "#####", ".....", "....."], // E
    ["#####", "#....", "#....", "####.", "#....", "#....", "#....", ".....", "....."], // F
    [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####", ".....", "....."], // G
    ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#", ".....", "....."], // H
    [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."], // I
    ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##..", ".....", "....."], // J
    ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#", ".....", "....."], // K
    ["#....", "#....", "#....", "#....", "#....", "#....", "#####", ".....", "....."], // L
    ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#", ".....", "....."], // M
    ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", ".....", "....."], // N
    [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.", ".....", "....."], // O
    ["####.", "#...#", "#...#", "####.", "#....", "#....", "#....", ".....", "....."], // P
    [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#", ".....", "....."], // Q
    ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#", ".....", "....."], // R
    [".####", "#....", "#....", ".###.", "....#", "....#", "####.", ".....", "....."], // S
    ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..", ".....", "....."], // T
    ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.", ".....", "....."], // U
    ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#..", ".....", "....."], // V
    ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#.", ".....", "....."], // W
    ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#", ".....", "....."], // X
    ["#...#", "#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", ".....", "....."], // Y
    ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####", ".....", "....."], // Z
    [".###.", ".#...", ".#...", ".#...", ".#...", ".#...", ".###.", ".....", "....."], // [
    [".....", "#....", ".#...", "..#..", "...#.", "....#", ".....", ".....", "....."], // backslash
    [".###.", "...#.", "...#.", "...#.", "...#.", "...#.", ".###.", ".....", "....."], // ]
    ["..#..", ".#.#.", "#...#", ".....", ".....", ".....", ".....", ".....", "....."], // ^
    [".....", ".....", ".....", ".....", ".....", ".....", "#####", ".....", "....."], // _
    [".#...", "..#..", "...#.", ".....", ".....", ".....", ".....", ".....", "....."], // `
    [".....", ".....", ".###.", "....#", ".####", "#...#", ".####", ".....", "....."], // a
    ["#....", "#....", "#.##.", "##..#", "#...#", "#...#", "####.", ".....", "....."], // b
    [".....", ".....", ".###.", "#....", "#....", "#...#", ".###.", ".....", "....."], // c
    ["....#", "....#", ".##.#", "#..##", "#...#", "#...#", ".####", ".....", "....."], // d
    [".....", ".....", ".###.", "#...#", "#####", "#....", ".###.", ".....", "....."], // e
    ["..##.", ".#..#", ".#...", "###..", ".#...", ".#...", ".#...", ".....", "....."], // f
    [".....", ".....", ".####", "#...#", "#...#", "#...#", ".####", "....#", ".###."], // g
    ["#....", "#....", "#.##.", "##..#", "#...#", "#...#", "#...#", ".....", "....."], // h
    ["..#..", ".....", ".##..", "..#..", "..#..", "..#..", ".###.", ".....", "....."], // i
    ["...#.", ".....", "..##.", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."], // j
    ["#....", "#....", "#..#.", "#.#..", "##...", "#.#..", "#..#.", ".....", "....."], // k
    [".##..", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."], // l
    [".....", ".....", "##.#.", "#.#.#", "#.#.#", "#...#", "#...#", ".....", "....."], // m
    [".....", ".....", "#.##.", "##..#", "#...#", "#...#", "#...#", ".....", "....."], // n
    [".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###.", ".....", "....."], // o
    [".....", ".....", "####.", "#...#", "#...#", "#...#", "####.", "#....", "#...."], // p
    [".....", ".....", ".####", "#...#", "#...#", "#...#", ".####", "....#", "....#"], // q
    [".....", ".....", "#.##.", "##..#", "#....", "#....", "#....", ".....", "....."], // r
    [".....", ".....", ".###.", "#....", ".###.", "....#", "####.", ".....", "....."], // s
    [".#...", ".#...", "###..", ".#...", ".#...", ".#..#", "..##.", ".....", "....."], // t
    [".....", ".....", "#...#", "#...#", "#...#", "#..##", ".##.#", ".....", "....."], // u
    [".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#..", ".....", "....."], // v
    [".....", ".....", "#...#", "#...#", "#.#.#", "#.#.#", ".#.#.", ".....", "....."], // w
    [".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", ".....", "....."], // x
    [".....", ".....", "#...#", "#...#", "#...#", "#...#", ".####", "....#", ".###."], // y
    [".....", ".....", "#####", "...#.", "..#..", ".#...", "#####", ".....", "....."], // z
    ["...#.", "..#..", "..#..", ".#...", "..#..", "..#..", "...#.", ".....", "....."], // {
    ["..#..", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..", ".....", "....."], // |
    [".#...", "..#..", "..#..", "...#.", "..#..", "..#..", ".#...", ".....", "....."], // }
    [".....", ".....", ".#...", "#.#.#", "...#.", ".....", ".....", ".....", "....."], // ~
]);

/// Whether the table draws `character`.
#[must_use]
pub(super) fn drawable(character: char) -> bool {
    (FIRST..=LAST).contains(&character)
}

/// The rows of `character`, or `None` outside printable ASCII.
#[must_use]
pub(super) fn rows(character: char) -> Option<&'static [u8; ROWS]> {
    drawable(character).then(|| &GLYPHS[character as usize - FIRST as usize])
}

/// The character that starts an escape; shown doubled ([`ESCAPED_BACKSLASH`]).
const BACKSLASH: char = '\\';
/// How the prompt shows a backslash of the text itself.
const ESCAPED_BACKSLASH: &str = "\\\\";
/// The character that ends a Unicode escape.
const ESCAPE_END: u8 = b'}';
/// The most characters one unit of shown text takes: the escape of the
/// largest code point, `\u{10ffff}`.
pub(super) const MAX_UNIT_CHARS: usize = 10;

/// `text` with every character the table cannot draw replaced by its
/// Unicode escape and every backslash doubled, so the prompt shows exactly
/// which code points it holds and no escape can be imitated.
#[must_use]
pub(super) fn visible(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    for character in text.chars() {
        if character == BACKSLASH {
            shown.push_str(ESCAPED_BACKSLASH);
        } else if drawable(character) {
            shown.push(character);
        } else {
            shown.extend(character.escape_unicode());
        }
    }
    shown
}

/// The units of `shown` ([`visible`] output) in order: one drawable
/// character, a doubled backslash or one whole `\u{...}` escape. A line
/// break between units never splits an escape.
#[must_use]
pub(super) fn units(shown: &str) -> Vec<&str> {
    let bytes = shown.as_bytes();
    let mut units = Vec::new();
    let mut start = 0;
    while start < bytes.len() {
        let length = if bytes[start] == BACKSLASH as u8 {
            if bytes.get(start + 1) == Some(&(BACKSLASH as u8)) {
                ESCAPED_BACKSLASH.len()
            } else {
                bytes[start..]
                    .iter()
                    .position(|byte| *byte == ESCAPE_END)
                    .map_or(1, |end| end + 1)
            }
        } else {
            shown[start..].chars().next().map_or(1, char::len_utf8)
        };
        units.push(&shown[start..start + length]);
        start += length;
    }
    units
}

/// The byte length of the longest prefix of `shown` ([`visible`] output)
/// made of whole [`units`] and at most `width` characters; at least its
/// first unit, so a caller always makes progress.
#[must_use]
pub(super) fn fit(shown: &str, width: usize) -> usize {
    let mut length = 0;
    for unit in units(shown) {
        if length > 0 && length + unit.len() > width {
            break;
        }
        length += unit.len();
    }
    length
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printable_ascii_is_drawn_and_everything_else_is_escaped() {
        assert!(rows(FIRST).is_some_and(|glyph| glyph.iter().all(|row| *row == 0)));
        for outside in ['\u{7f}', '\n', '\u{430}', '\u{200b}', '\u{1F600}'] {
            assert!(rows(outside).is_none(), "{outside:?}");
        }
        // A Cyrillic `a` and a zero-width space never pass for ASCII.
        assert_eq!(
            visible("p\u{430}y\u{200b}.test"),
            "p\\u{430}y\\u{200b}.test"
        );
        assert!(visible("\u{1F600}").chars().all(drawable));
        // A text that spells an escape is not one: its backslash is doubled.
        assert_eq!(visible("p\\u{430}y"), "p\\\\u{430}y");
        assert_ne!(visible("p\\u{430}y"), visible("p\u{430}y"));
        assert_eq!(
            units(&visible("a\\\u{430}\u{10ffff}b")),
            ["a", "\\\\", "\\u{430}", "\\u{10ffff}", "b"]
        );
        assert_eq!(char::MAX.escape_unicode().count(), MAX_UNIT_CHARS);
        // A cut keeps whole units, and always at least the first one.
        let shown = visible("ab\u{430}c");
        assert_eq!(fit(&shown, 4), "ab".len());
        assert_eq!(fit(&shown, 9), "ab\\u{430}".len());
        assert_eq!(fit(&visible("\u{430}"), 1), "\\u{430}".len());
        // Every visible glyph lights at least one pixel and no row uses
        // more than the cell's columns.
        for character in '!'..=LAST {
            let glyph = rows(character).unwrap();
            assert!(glyph.iter().any(|row| *row != 0), "{character:?}");
            assert!(glyph.iter().all(|row| *row >> COLUMNS == 0));
        }
        // Only the descending letters reach below the baseline.
        for character in FIRST..=LAST {
            let descends = rows(character).unwrap()[CAP_ROWS..]
                .iter()
                .any(|row| *row != 0);
            assert_eq!(descends, "gjpqy".contains(character), "{character:?}");
        }
        // Distinct characters have distinct glyphs (the table is in code
        // point order, with no duplicated or shifted row).
        for (index, glyph) in GLYPHS.iter().enumerate() {
            assert!(
                GLYPHS[index + 1..].iter().all(|other| other != glyph),
                "{:?}",
                char::from_u32(FIRST as u32 + index as u32)
            );
        }
    }
}
