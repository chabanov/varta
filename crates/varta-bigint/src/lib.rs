#![forbid(unsafe_code)]
//! Exact integer semantics and canonical transport for checked guest BigInt.
//! Resource policy is supplied by callers; this crate does not raise VM budgets.
mod arithmetic;
mod convert;
mod parse;
mod transport;
use num_bigint::BigInt;
use num_traits::Zero;
pub use varta_common::bigint::{BinaryOp, UnaryOp};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("noncanonical BigInt binary payload")]
    InvalidPayload,
    #[error("invalid BigInt string")]
    Syntax,
    #[error("BigInt division by zero")]
    DivisionByZero,
    #[error("BigInt exponent must be nonnegative")]
    NegativeExponent,
    #[error("Number cannot be converted to BigInt")]
    NonIntegralNumber,
    #[error("BigInt radix must be in 2..=36")]
    InvalidRadix,
    #[error("BigInt bit width must be a valid index")]
    InvalidWidth,
    #[error("unary plus and unsigned right shift do not accept BigInt")]
    Type,
    #[error("BigInt result exceeds caller bit capacity")]
    BitCapacity,
    #[error("BigInt work budget exhausted")]
    WorkBudget,
}
/// A fatal resource error must stay distinct from recoverable JS errors.
impl Error {
    /// Only declared semantic errors can enter the guest exception channel.
    pub fn recoverable_class(self) -> Option<varta_common::value::ErrorClass> {
        use varta_common::value::ErrorClass;
        Some(match self {
            Self::Syntax => ErrorClass::SyntaxError,
            Self::Type => ErrorClass::TypeError,
            Self::DivisionByZero
            | Self::NegativeExponent
            | Self::NonIntegralNumber
            | Self::InvalidRadix
            | Self::InvalidWidth => ErrorClass::RangeError,
            Self::InvalidPayload | Self::BitCapacity | Self::WorkBudget => return None,
        })
    }
    pub fn is_resource(self) -> bool {
        matches!(self, Self::BitCapacity | Self::WorkBudget)
    }
}
#[derive(Debug, Clone)]
pub struct Budget {
    max_result_bits: u64,
    initial_work: u64,
    remaining_work: u64,
}
impl Budget {
    pub fn new(max_result_bits: u64, work: u64) -> Self {
        Self {
            max_result_bits,
            initial_work: work,
            remaining_work: work,
        }
    }
    /// Account for linear bridge work such as encoded result transfer.
    pub fn consume_work(&mut self, units: u64) -> Result<(), Error> {
        self.charge(units)
    }
    pub fn remaining_work(&self) -> u64 {
        self.remaining_work
    }
    pub fn spent_work(&self) -> u64 {
        self.initial_work - self.remaining_work
    }
    pub fn max_result_bits(&self) -> u64 {
        self.max_result_bits
    }
    fn charge(&mut self, units: u64) -> Result<(), Error> {
        match self.remaining_work.checked_sub(units) {
            Some(left) => {
                self.remaining_work = left;
                Ok(())
            }
            None => {
                self.remaining_work = 0;
                Err(Error::WorkBudget)
            }
        }
    }
    fn capacity(&self, bits: u64) -> Result<(), Error> {
        if bits > self.max_result_bits
            || usize::try_from(bits.div_ceil(8)).is_err()
            || bits.div_ceil(8) > isize::MAX as u64
        {
            Err(Error::BitCapacity)
        } else {
            Ok(())
        }
    }
    fn finish(&self, value: BigInt) -> Result<Integer, Error> {
        self.capacity(value.bits())?;
        Ok(Integer(value))
    }
}
/// Values are immutable mathematical integers; zero has one canonical value.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Integer(BigInt);
fn words(bits: u64) -> u64 {
    bits.div_ceil(64).max(1)
}
impl Integer {
    pub fn bits(&self) -> u64 {
        self.0.bits()
    }
    pub fn is_zero(&self) -> bool {
        self.0.is_zero()
    }
    pub fn from_i64(value: i64, budget: &mut Budget) -> Result<Self, Error> {
        budget.charge(1)?;
        budget.finish(BigInt::from(value))
    }
    pub fn from_bool(value: bool, budget: &mut Budget) -> Result<Self, Error> {
        Self::from_i64(i64::from(value), budget)
    }
    pub fn compare(&self, other: &Self, budget: &mut Budget) -> Result<std::cmp::Ordering, Error> {
        budget.charge(words(self.bits().max(other.bits())))?;
        Ok(self.0.cmp(&other.0))
    }
}
