use super::*;
use num_bigint::Sign;
use num_traits::{One, ToPrimitive};
impl Integer {
    pub fn unary(&self, op: UnaryOp, budget: &mut Budget) -> Result<Self, Error> {
        budget.charge(words(self.bits()))?;
        match op {
            UnaryOp::Negate => budget.finish(-&self.0),
            UnaryOp::Not => budget.finish(!&self.0),
            UnaryOp::Plus => Err(Error::Type),
        }
    }
    pub fn binary(&self, op: BinaryOp, other: &Self, budget: &mut Budget) -> Result<Self, Error> {
        match op {
            BinaryOp::Power => return self.power(other, budget),
            BinaryOp::LeftShift | BinaryOp::RightShift => {
                return self.shift(other, op == BinaryOp::LeftShift, budget);
            }
            BinaryOp::UnsignedRightShift => {
                budget.charge(1)?;
                return Err(Error::Type);
            }
            _ => {}
        }
        let a = words(self.bits());
        let b = words(other.bits());
        let work = if matches!(
            op,
            BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Remainder
        ) {
            a.checked_mul(b).ok_or(Error::WorkBudget)?
        } else {
            a.max(b)
        };
        budget.charge(work)?;
        if matches!(op, BinaryOp::Divide | BinaryOp::Remainder) && other.is_zero() {
            return Err(Error::DivisionByZero);
        }
        if op == BinaryOp::Multiply && !self.is_zero() && !other.is_zero() {
            budget.capacity(self.bits().saturating_add(other.bits()).saturating_sub(1))?;
        }
        let value = match op {
            BinaryOp::Add => &self.0 + &other.0,
            BinaryOp::Subtract => &self.0 - &other.0,
            BinaryOp::Multiply => &self.0 * &other.0,
            BinaryOp::Divide => &self.0 / &other.0,
            BinaryOp::Remainder => &self.0 % &other.0,
            BinaryOp::And => &self.0 & &other.0,
            BinaryOp::Or => &self.0 | &other.0,
            BinaryOp::Xor => &self.0 ^ &other.0,
            _ => unreachable!(),
        };
        budget.finish(value)
    }
    fn power(&self, exponent: &Self, budget: &mut Budget) -> Result<Self, Error> {
        budget.charge(words(self.bits()).saturating_add(words(exponent.bits())))?;
        if exponent.0.sign() == Sign::Minus {
            return Err(Error::NegativeExponent);
        }
        if exponent.is_zero() {
            return budget.finish(BigInt::one());
        }
        if self.is_zero() {
            return budget.finish(BigInt::zero());
        }
        if self.0 == BigInt::one() {
            return budget.finish(BigInt::one());
        }
        if self.0 == -BigInt::one() {
            return budget.finish(if exponent.0.magnitude().bit(0) {
                -BigInt::one()
            } else {
                BigInt::one()
            });
        }
        let mut exponent = exponent.0.to_u64().ok_or(Error::BitCapacity)?;
        budget.capacity(
            self.bits()
                .saturating_sub(1)
                .saturating_mul(exponent)
                .saturating_add(1),
        )?;
        let mut base = self.clone();
        let mut result = Self(BigInt::one());
        while exponent > 0 {
            if exponent & 1 != 0 {
                result = result.binary(BinaryOp::Multiply, &base, budget)?;
            }
            exponent >>= 1;
            if exponent > 0 {
                base = base.binary(BinaryOp::Multiply, &base, budget)?;
            }
        }
        Ok(result)
    }
    fn shift(&self, count: &Self, mut left: bool, budget: &mut Budget) -> Result<Self, Error> {
        budget.charge(words(self.bits()).saturating_add(words(count.bits())))?;
        if self.is_zero() {
            return budget.finish(BigInt::zero());
        }
        if count.0.sign() == Sign::Minus {
            left = !left;
        }
        let amount = count.0.magnitude().to_u64();
        if !left && amount.is_none_or(|a| a >= self.bits()) {
            return budget.finish(if self.0.sign() == Sign::Minus {
                -BigInt::one()
            } else {
                BigInt::zero()
            });
        }
        let amount = amount.ok_or(Error::BitCapacity)?;
        if left {
            let bits = self.bits().checked_add(amount).ok_or(Error::BitCapacity)?;
            budget.capacity(bits)?;
            // A tiny operand can grow into a large result. Charge that work
            // before num-bigint allocates or initializes the destination digits.
            budget.charge(words(bits))?;
        }
        let amount = usize::try_from(amount).map_err(|_| Error::BitCapacity)?;
        budget.finish(if left {
            &self.0 << amount
        } else {
            &self.0 >> amount
        })
    }
}
