//! Budgeted conversion to the canonical guest binary payload; no decimal bridge.
use super::*;
use num_bigint::Sign as NumericSign;
use varta_common::bigint::{BigIntView, DIGIT_BYTES, MAX_PAYLOAD_BYTES, META_BYTES, Sign};

impl Integer {
    /// Import canonical unsigned base-2^32 digits, e.g. from a parsed literal.
    pub fn from_unsigned_digits(digits: &[u32], budget: &mut Budget) -> Result<Self, Error> {
        if digits.last() == Some(&0) {
            return Err(Error::InvalidPayload);
        }
        let bits = digits.last().map_or(0, |last| {
            (digits.len() as u64) * 32 - u64::from(last.leading_zeros())
        });
        budget.charge(words(bits))?;
        budget.capacity(bits)?;
        budget.finish(BigInt::from_slice(NumericSign::Plus, digits))
    }

    /// Malformed transport is an ABI error, never a recoverable SyntaxError.
    pub fn from_payload(payload: &[u8], budget: &mut Budget) -> Result<Self, Error> {
        let view = BigIntView::from_payload(payload).map_err(|_| Error::InvalidPayload)?;
        budget.charge(words(view.bits()))?;
        budget.capacity(view.bits())?;
        let sign = match view.sign() {
            Sign::Zero => NumericSign::NoSign,
            Sign::Positive => NumericSign::Plus,
            Sign::Negative => NumericSign::Minus,
        };
        budget.finish(BigInt::from_bytes_le(sign, view.magnitude()))
    }
    /// Result bytes are snapshotted into a rooted immutable guest leaf by callers.
    pub fn to_payload(&self, budget: &mut Budget) -> Result<Vec<u8>, Error> {
        budget.capacity(self.bits())?;
        let digits = usize::try_from(self.bits().div_ceil(u64::from(u32::BITS)))
            .map_err(|_| Error::BitCapacity)?;
        let length = digits
            .checked_mul(DIGIT_BYTES)
            .and_then(|n| n.checked_add(META_BYTES))
            .filter(|n| *n <= MAX_PAYLOAD_BYTES)
            .ok_or(Error::BitCapacity)?;
        budget.charge(words(self.bits()))?;
        let sign = match self.0.sign() {
            NumericSign::NoSign => Sign::Zero,
            NumericSign::Plus => Sign::Positive,
            NumericSign::Minus => Sign::Negative,
        };
        let mut payload = Vec::with_capacity(length);
        payload.extend_from_slice(&(sign as u32).to_le_bytes());
        payload.extend_from_slice(&(digits as u32).to_le_bytes());
        // Iterating base-2^32 digits avoids a second large temporary byte vector.
        for digit in self.0.magnitude().iter_u32_digits() {
            payload.extend_from_slice(&digit.to_le_bytes());
        }
        payload.resize(length, 0);
        debug_assert!(BigIntView::from_payload(&payload).is_ok());
        Ok(payload)
    }
}
