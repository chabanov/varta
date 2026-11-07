//! Hand-assembled programs: the matcher's own contract. Patterns compiled
//! from source are tested through the compiler and the runtime against Node.
use super::*;
use alloc::{vec, vec::Vec};
use varta_common::regexp::header;

fn bytes(units: &[u16]) -> Vec<u8> {
    units.iter().flat_map(|unit| unit.to_le_bytes()).collect()
}
fn text(text: &str) -> Vec<u8> {
    bytes(&text.encode_utf16().collect::<Vec<_>>())
}
/// A program with `groups` capture groups, `registers` registers and `code`
/// placed after the header (offsets in `code` are relative to its start).
fn program(groups: u16, registers: u16, flags: u16, code: &[u16]) -> Vec<u8> {
    let base = header::UNITS as u16;
    let mut units = vec![0; header::UNITS];
    units[header::FORMAT] = FORMAT_VERSION;
    units[header::FLAGS] = flags;
    units[header::CAPTURES] = 2 * (groups + 1);
    units[header::REGISTERS] = registers;
    units[header::CODE] = base;
    units.extend_from_slice(code);
    bytes(&units)
}
const B: u16 = header::UNITS as u16;

#[test]
fn literal_search_and_sticky_start() {
    // /bc/
    let p = program(
        0,
        0,
        0,
        &[op::CHAR, 'b' as u16, op::CHAR, 'c' as u16, op::MATCH],
    );
    assert_eq!(exec(&p, &text("abcbc"), 0.0, false), Ok(Some(vec![1, 3])));
    assert_eq!(exec(&p, &text("abcbc"), 2.0, false), Ok(Some(vec![3, 5])));
    assert_eq!(exec(&p, &text("abcbc"), 2.0, true), Ok(None));
    assert_eq!(exec(&p, &text("abcbc"), 6.0, false), Ok(None));
}

#[test]
fn backreference_and_greedy_star_backtrack() {
    // /(a*)b\1/ : SAVE 2, STAR a, SAVE 3, CHAR b, BACKREF 1, MATCH
    let code = [
        op::SAVE,
        2,
        op::STAR,
        0,
        0,
        u16::MAX,
        u16::MAX,
        1,
        op::CHAR,
        'a' as u16,
        op::SAVE,
        3,
        op::CHAR,
        'b' as u16,
        op::BACKREF,
        0,
        1,
        1,
        op::MATCH,
    ];
    let p = program(1, 0, 0, &code);
    assert_eq!(
        exec(&p, &text("aaba"), 0.0, false),
        Ok(Some(vec![1, 4, 1, 2]))
    );
}

#[test]
fn negative_lookahead_restores_captures() {
    // /(?!(a)b)a/ on "ab a": the lookahead continues at the `CHAR` after
    // its `LOOK_END` (unit 12 of the code).
    let code = [
        op::LOOK,
        look::NEGATIVE,
        B + 12,
        op::SAVE,
        2,
        op::CHAR,
        'a' as u16,
        op::SAVE,
        3,
        op::CHAR,
        'b' as u16,
        op::LOOK_END,
        op::CHAR,
        'a' as u16,
        op::MATCH,
    ];
    let p = program(1, 0, 0, &code);
    assert_eq!(
        exec(&p, &text("ab a"), 0.0, false),
        Ok(Some(vec![3, 4, -1, -1]))
    );
}

#[test]
fn malformed_programs_are_errors() {
    assert_eq!(
        exec(&[], &text("a"), 0.0, false),
        Err(IntrinsicError::RegExpProgram)
    );
    let p = program(0, 0, 0, &[0x77]);
    assert_eq!(
        exec(&p, &text("a"), 0.0, false),
        Err(IntrinsicError::RegExpProgram)
    );
}

#[test]
fn canonicalize_follows_both_modes() {
    assert_eq!(canonicalize(u32::from('a'), false), u32::from('A'));
    assert_eq!(canonicalize(0x017F, false), 0x017F);
    assert_eq!(canonicalize(0x017F, true), u32::from('s'));
    assert_eq!(canonicalize(0x212A, true), u32::from('k'));
    assert_eq!(canonicalize(0x0130, true), 0x0130);
}
