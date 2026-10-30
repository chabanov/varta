//! Fatal intrinsic failures. Each is an execution trap in every host: the
//! native runtime reports the message, linked guest code traps.

/// Why an intrinsic refused its input. None of these is a recoverable status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IntrinsicError {
    #[error("noncanonical decimal precision")]
    NoncanonicalPrecision,
    #[error("invalid fixed precision")]
    FixedPrecision,
    #[error("invalid decimal precision")]
    DecimalPrecision,
    #[error("decimal text bound exceeded")]
    DecimalTextBound,
    #[error("noncanonical numeric radix")]
    NoncanonicalRadix,
    #[error("numeric radix text bound exceeded")]
    RadixTextBound,
    #[error("numeric radix precision bound exceeded")]
    RadixPrecisionBound,
    #[error("noncanonical primitive union")]
    PrimitiveCell,
    #[error("numeric parsing requires primitive cells")]
    ParseCell,
    #[error("Number conversion requires a primitive cell")]
    NumberCell,
    #[error("String conversion requires a primitive value cell")]
    StringCell,
    #[error("invalid UTF-16 range")]
    Utf16Range,
    #[error("UTF-16 range outside memory")]
    Utf16Memory,
    #[error("invalid string encoding")]
    Utf16Length,
    #[error("case conversion exceeds the string limit")]
    CaseLimit,
    #[error("result too large")]
    ResultTooLarge,
}
