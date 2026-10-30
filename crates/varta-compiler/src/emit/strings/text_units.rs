//! UTF-16 code-unit scans and ASCII lowercasing for the v35 Web type
//! preludes (`ir::TextUnits`). They read the immutable string in place, make
//! no host call and allocate only the lowercased copy.
use super::*;
use crate::ir::UnitClass;
const INPUT: u32 = 0;
const PTR: u32 = 1;
const LENGTH: u32 = 2;
const AT: u32 = 3;
const UNIT: u32 = 4;
const BYTES: u32 = 1;
const DEST: u32 = 2;
const UNIT_BYTES: i32 = size_of::<u16>() as i32;
const UNIT_MEMORY: MemArg = MemArg {
    offset: 0,
    align: 1,
    memory_index: MEMORY,
};
/// Code units below this are ASCII; each half has a 64-bit membership mask.
const ASCII_END: u32 = 128;
const HALF: u32 = u64::BITS;
/// Above every UTF-16 code unit: the class matches no unit by threshold.
const NO_THRESHOLD: u32 = 0x1_0000;
/// The first code unit above the WebIDL ByteString range.
const ABOVE_BYTE: u32 = 0x100;
/// RFC 9110 `tchar` symbols; ASCII letters and digits are tokens too.
const TOKEN_SYMBOLS: &str = "!#$%&'*+-.^_`|~";
const FORBIDDEN_VALUE: [u8; 3] = [0x00, b'\n', b'\r'];
const UPPERCASE_FIRST: i32 = b'A' as i32;
const LOWERCASE_FIRST: i32 = b'a' as i32;
const ALPHABET: i32 = 26;
const CASE_OFFSET: i32 = (b'a' - b'A') as i32;

/// `unit >= threshold`, or an ASCII unit whose bit is set in `low`/`high`.
struct Matcher {
    low: u64,
    high: u64,
    threshold: u32,
}
fn ascii(member: impl Fn(u8) -> bool) -> (u64, u64) {
    (0..ASCII_END as u8)
        .filter(|unit| member(*unit))
        .fold((0, 0), |(low, high), unit| {
            let unit = u32::from(unit);
            if unit < HALF {
                (low | 1 << unit, high)
            } else {
                (low, high | 1 << (unit - HALF))
            }
        })
}
fn matcher(class: UnitClass) -> Matcher {
    match class {
        UnitClass::AboveByte => Matcher {
            low: 0,
            high: 0,
            threshold: ABOVE_BYTE,
        },
        UnitClass::NonToken => {
            let (low, high) = ascii(|unit| {
                !(unit.is_ascii_alphanumeric() || TOKEN_SYMBOLS.as_bytes().contains(&unit))
            });
            Matcher {
                low,
                high,
                threshold: ASCII_END,
            }
        }
        UnitClass::ForbiddenValue => {
            let (low, high) = ascii(|unit| FORBIDDEN_VALUE.contains(&unit));
            Matcher {
                low,
                high,
                threshold: NO_THRESHOLD,
            }
        }
        UnitClass::NonAscii => Matcher {
            low: 0,
            high: 0,
            threshold: ASCII_END,
        },
    }
}

/// `(input: string) -> f64`: the index of the first unit of `class`, or -1.
pub(super) fn index(class: UnitClass) -> Function {
    let Matcher {
        low,
        high,
        threshold,
    } = matcher(class);
    let mut f = Function::new([(UNIT - PTR + 1, ValType::I32)]);
    instructions(
        &mut f,
        [I::LocalGet(INPUT), I::I32WrapI64, I::LocalSet(PTR)],
    );
    length(&mut f, INPUT, LENGTH);
    instructions(
        &mut f,
        [
            I::LocalGet(LENGTH),
            I::I32Const(UNIT_BYTES),
            I::I32DivU,
            I::LocalSet(LENGTH),
            I::I32Const(0),
            I::LocalSet(AT),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(AT),
            I::LocalGet(LENGTH),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(PTR),
            I::LocalGet(AT),
            I::I32Const(UNIT_BYTES),
            I::I32Mul,
            I::I32Add,
            I::I32Load16U(UNIT_MEMORY),
            I::LocalSet(UNIT),
        ],
    );
    if threshold < NO_THRESHOLD {
        instructions(
            &mut f,
            [
                I::LocalGet(UNIT),
                I::I32Const(threshold as i32),
                I::I32GeU,
                I::If(BlockType::Empty),
                I::LocalGet(AT),
                I::F64ConvertI32U,
                I::Return,
                I::End,
            ],
        );
    }
    for (mask, start) in [(low, 0), (high, HALF)] {
        if mask == 0 {
            continue;
        }
        // start <= UNIT < start + 64 and bit (UNIT - start) of the mask.
        instructions(
            &mut f,
            [
                I::LocalGet(UNIT),
                I::I32Const(start as i32),
                I::I32Sub,
                I::I32Const(HALF as i32),
                I::I32LtU,
                I::If(BlockType::Empty),
                I::I64Const(mask as i64),
                I::LocalGet(UNIT),
                I::I32Const(start as i32),
                I::I32Sub,
                I::I64ExtendI32U,
                I::I64ShrU,
                I::I64Const(1),
                I::I64And,
                I::I32WrapI64,
                I::If(BlockType::Empty),
                I::LocalGet(AT),
                I::F64ConvertI32U,
                I::Return,
                I::End,
                I::End,
            ],
        );
    }
    instructions(
        &mut f,
        [
            I::LocalGet(AT),
            I::I32Const(1),
            I::I32Add,
            I::LocalSet(AT),
            I::Br(0),
            I::End,
            I::End,
            I::F64Const((-1.0).into()),
            I::End,
        ],
    );
    f
}

/// `(input: string) -> string`: a copy with ASCII `A`-`Z` lowercased, or
/// (`upper`) with ASCII `a`-`z` uppercased.
pub(super) fn ascii_case(allocate: u32, upper: bool) -> Function {
    let (first, offset) = if upper {
        (LOWERCASE_FIRST, -CASE_OFFSET)
    } else {
        (UPPERCASE_FIRST, CASE_OFFSET)
    };
    let mut f = Function::new([(UNIT - BYTES + 1, ValType::I32)]);
    length(&mut f, INPUT, BYTES);
    instructions(
        &mut f,
        [
            I::LocalGet(BYTES),
            I::I32Eqz,
            I::If(BlockType::Empty),
            I::I64Const(0),
            I::Return,
            I::End,
            I::LocalGet(BYTES),
            I::Call(allocate),
            I::LocalTee(DEST),
            I::LocalGet(INPUT),
            I::I32WrapI64,
            I::LocalGet(BYTES),
            I::MemoryCopy {
                src_mem: MEMORY,
                dst_mem: MEMORY,
            },
            I::I32Const(0),
            I::LocalSet(AT),
            I::Block(BlockType::Empty),
            I::Loop(BlockType::Empty),
            I::LocalGet(AT),
            I::LocalGet(BYTES),
            I::I32GeU,
            I::BrIf(1),
            I::LocalGet(DEST),
            I::LocalGet(AT),
            I::I32Add,
            I::I32Load16U(UNIT_MEMORY),
            I::LocalTee(UNIT),
            I::I32Const(first),
            I::I32Sub,
            I::I32Const(ALPHABET),
            I::I32LtU,
            I::If(BlockType::Empty),
            I::LocalGet(DEST),
            I::LocalGet(AT),
            I::I32Add,
            I::LocalGet(UNIT),
            I::I32Const(offset),
            I::I32Add,
            I::I32Store16(UNIT_MEMORY),
            I::End,
            I::LocalGet(AT),
            I::I32Const(UNIT_BYTES),
            I::I32Add,
            I::LocalSet(AT),
            I::Br(0),
            I::End,
            I::End,
        ],
    );
    pack(&mut f, BYTES, DEST);
    f.instruction(&I::End);
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    fn matches(class: UnitClass, unit: u32) -> bool {
        let Matcher {
            low,
            high,
            threshold,
        } = matcher(class);
        unit >= threshold
            || unit < HALF && low >> unit & 1 == 1
            || (HALF..ASCII_END).contains(&unit) && high >> (unit - HALF) & 1 == 1
    }
    #[test]
    fn unit_classes_match_their_definitions() {
        for unit in 0..NO_THRESHOLD {
            let token = char::from_u32(unit).is_some_and(|c| {
                c.is_ascii_alphanumeric() || c.is_ascii() && TOKEN_SYMBOLS.contains(c)
            });
            assert_eq!(matches(UnitClass::NonToken, unit), !token, "{unit:#x}");
            assert_eq!(
                matches(UnitClass::AboveByte, unit),
                unit > 0xFF,
                "{unit:#x}"
            );
            assert_eq!(
                matches(UnitClass::ForbiddenValue, unit),
                matches!(unit, 0x00 | 0x0A | 0x0D),
                "{unit:#x}"
            );
            assert_eq!(matches(UnitClass::NonAscii, unit), unit > 0x7F, "{unit:#x}");
        }
    }
}
