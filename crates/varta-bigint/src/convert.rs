use super::*;
use num_bigint::{BigUint, Sign};
use num_traits::{One, ToPrimitive};
use std::cmp::Ordering;
const MAX_INDEX: u64 = (1 << 53) - 1;
impl Integer {
    pub fn from_number(value: f64, budget: &mut Budget) -> Result<Self, Error> {
        budget.charge(1)?;
        if !value.is_finite() || value.fract() != 0.0 {
            return Err(Error::NonIntegralNumber);
        }
        if value == 0.0 {
            return budget.finish(BigInt::zero());
        }
        let raw = value.to_bits();
        let exponent = ((raw >> 52) & 0x7ff) as i32 - 1023;
        budget.capacity((exponent + 1) as u64)?;
        budget.charge(words((exponent + 1) as u64))?;
        let mantissa = (1u64 << 52) | (raw & ((1u64 << 52) - 1));
        let magnitude = if exponent >= 52 {
            BigInt::from(mantissa) << ((exponent - 52) as usize)
        } else {
            BigInt::from(mantissa) >> ((52 - exponent) as usize)
        };
        budget.finish(if value.is_sign_negative() {
            -magnitude
        } else {
            magnitude
        })
    }
    pub fn to_number(&self, budget: &mut Budget) -> Result<f64, Error> {
        budget.charge(words(self.bits()))?;
        let magnitude = self.0.magnitude();
        let bits = self.bits();
        let value = if bits <= 53 {
            magnitude.to_u64().unwrap_or(0) as f64
        } else if bits > 1024 {
            f64::INFINITY
        } else {
            let shift = bits - 53;
            let mut leading = (magnitude >> (shift as usize)).to_u64().expect("53 bits");
            let guard = magnitude.bit(shift - 1);
            let sticky = magnitude.trailing_zeros().is_some_and(|n| n < shift - 1);
            if guard && (sticky || leading & 1 != 0) {
                leading += 1;
            }
            (leading as f64) * 2.0f64.powi(shift as i32)
        };
        Ok(if self.0.sign() == Sign::Minus {
            -value
        } else {
            value
        })
    }
    /// Exact mathematical comparison; None is unordered for a NaN Number.
    pub fn compare_number(
        &self,
        value: f64,
        budget: &mut Budget,
    ) -> Result<Option<Ordering>, Error> {
        budget.charge(words(self.bits()).saturating_add(1))?;
        if value.is_nan() {
            return Ok(None);
        }
        if value.is_infinite() {
            return Ok(Some(if value.is_sign_positive() {
                Ordering::Less
            } else {
                Ordering::Greater
            }));
        }
        if self.is_zero() {
            return Ok(Some(if value == 0.0 {
                Ordering::Equal
            } else if value > 0.0 {
                Ordering::Less
            } else {
                Ordering::Greater
            }));
        }
        if value == 0.0 {
            return Ok(Some(if self.0.sign() == Sign::Minus {
                Ordering::Less
            } else {
                Ordering::Greater
            }));
        }
        let negative = self.0.sign() == Sign::Minus;
        if negative != value.is_sign_negative() {
            return Ok(Some(if negative {
                Ordering::Less
            } else {
                Ordering::Greater
            }));
        }
        let raw = value.to_bits();
        let exponent = ((raw >> 52) & 0x7ff) as i32 - 1023;
        let order = if exponent < 0 {
            Ordering::Greater
        } else if self.bits() != (exponent + 1) as u64 {
            self.bits().cmp(&((exponent + 1) as u64))
        } else {
            let mantissa = (1u64 << 52) | (raw & ((1u64 << 52) - 1));
            if exponent >= 52 {
                self.0
                    .magnitude()
                    .cmp(&(BigUint::from(mantissa) << ((exponent - 52) as usize)))
            } else {
                let shift = (52 - exponent) as u32;
                let integer = mantissa >> shift;
                let order = self.0.magnitude().cmp(&BigUint::from(integer));
                if order == Ordering::Equal && mantissa & ((1u64 << shift) - 1) != 0 {
                    Ordering::Less
                } else {
                    order
                }
            }
        };
        Ok(Some(if negative { order.reverse() } else { order }))
    }
    pub fn to_string_radix(&self, radix: u32, budget: &mut Budget) -> Result<String, Error> {
        if !(2..=36).contains(&radix) {
            budget.charge(1)?;
            return Err(Error::InvalidRadix);
        }
        let size = words(self.bits());
        budget.charge(size.checked_mul(size).ok_or(Error::WorkBudget)?)?;
        Ok(self.0.to_str_radix(radix))
    }
    pub fn index_from_number(value: f64) -> Result<u64, Error> {
        if value.is_nan() || value == 0.0 {
            return Ok(0);
        }
        let value = value.trunc();
        if !value.is_finite() || value < 0.0 || value > MAX_INDEX as f64 {
            return Err(Error::InvalidWidth);
        }
        Ok(value as u64)
    }
    fn unsigned(&self, bits: u64, budget: &mut Budget) -> Result<BigInt, Error> {
        if bits > MAX_INDEX {
            return Err(Error::InvalidWidth);
        }
        budget.charge(words(self.bits()))?;
        if bits == 0 || self.is_zero() {
            return Ok(BigInt::zero());
        }
        if self.0.sign() != Sign::Minus && bits >= self.bits() {
            return Ok(self.0.clone());
        }
        if self.0.sign() == Sign::Minus && bits >= self.bits() {
            budget.capacity(bits)?;
        }
        if self
            .0
            .magnitude()
            .trailing_zeros()
            .is_some_and(|n| n >= bits)
        {
            return Ok(BigInt::zero());
        }
        budget.charge(words(bits))?;
        let width = usize::try_from(bits).map_err(|_| Error::BitCapacity)?;
        let modulus = BigInt::one() << width;
        let mut value = &self.0 % &modulus;
        if value.sign() == Sign::Minus {
            value += modulus;
        }
        Ok(value)
    }
    pub fn as_uint_n(&self, bits: u64, budget: &mut Budget) -> Result<Self, Error> {
        let value = self.unsigned(bits, budget)?;
        budget.finish(value)
    }
    pub fn as_int_n(&self, bits: u64, budget: &mut Budget) -> Result<Self, Error> {
        if bits > MAX_INDEX {
            return Err(Error::InvalidWidth);
        }
        if bits > self.bits()
            || bits == self.bits()
                && self.0.sign() == Sign::Minus
                && self.0.magnitude().trailing_zeros() == bits.checked_sub(1)
        {
            budget.charge(words(self.bits()))?;
            return budget.finish(self.0.clone());
        }
        let mut value = self.unsigned(bits, budget)?;
        if bits > 0 && value.magnitude().bit(bits - 1) {
            budget.charge(words(bits))?;
            value -= BigInt::one() << (usize::try_from(bits).map_err(|_| Error::BitCapacity)?);
        }
        budget.finish(value)
    }
}
