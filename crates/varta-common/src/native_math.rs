//! Deterministic native Math inventory shared by compiler and runtime.
use crate::HostOperation;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeMath {
    Acos,
    Acosh,
    Asin,
    Asinh,
    Atan,
    Atanh,
    Atan2,
    Cbrt,
    Cos,
    Cosh,
    Exp,
    Expm1,
    Log,
    Log1p,
    Log10,
    Log2,
    Sin,
    Sinh,
    Tan,
    Tanh,
}

impl NativeMath {
    pub const ALL: &[Self] = &[
        Self::Acos,
        Self::Acosh,
        Self::Asin,
        Self::Asinh,
        Self::Atan,
        Self::Atanh,
        Self::Atan2,
        Self::Cbrt,
        Self::Cos,
        Self::Cosh,
        Self::Exp,
        Self::Expm1,
        Self::Log,
        Self::Log1p,
        Self::Log10,
        Self::Log2,
        Self::Sin,
        Self::Sinh,
        Self::Tan,
        Self::Tanh,
    ];
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Acos => "acos",
            Self::Acosh => "acosh",
            Self::Asin => "asin",
            Self::Asinh => "asinh",
            Self::Atan => "atan",
            Self::Atanh => "atanh",
            Self::Atan2 => "atan2",
            Self::Cbrt => "cbrt",
            Self::Cos => "cos",
            Self::Cosh => "cosh",
            Self::Exp => "exp",
            Self::Expm1 => "expm1",
            Self::Log => "log",
            Self::Log1p => "log1p",
            Self::Log10 => "log10",
            Self::Log2 => "log2",
            Self::Sin => "sin",
            Self::Sinh => "sinh",
            Self::Tan => "tan",
            Self::Tanh => "tanh",
        }
    }
    #[must_use]
    pub fn host(self) -> HostOperation {
        match self {
            Self::Acos => HostOperation::MathAcos,
            Self::Acosh => HostOperation::MathAcosh,
            Self::Asin => HostOperation::MathAsin,
            Self::Asinh => HostOperation::MathAsinh,
            Self::Atan => HostOperation::MathAtan,
            Self::Atanh => HostOperation::MathAtanh,
            Self::Atan2 => HostOperation::MathAtan2,
            Self::Cbrt => HostOperation::MathCbrt,
            Self::Cos => HostOperation::MathCos,
            Self::Cosh => HostOperation::MathCosh,
            Self::Exp => HostOperation::MathExp,
            Self::Expm1 => HostOperation::MathExpm1,
            Self::Log => HostOperation::MathLog,
            Self::Log1p => HostOperation::MathLog1p,
            Self::Log10 => HostOperation::MathLog10,
            Self::Log2 => HostOperation::MathLog2,
            Self::Sin => HostOperation::MathSin,
            Self::Sinh => HostOperation::MathSinh,
            Self::Tan => HostOperation::MathTan,
            Self::Tanh => HostOperation::MathTanh,
        }
    }
    #[must_use]
    pub fn arity(self) -> usize {
        if self == Self::Atan2 { 2 } else { 1 }
    }
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|operation| operation.name() == name)
    }
    #[must_use]
    pub fn from_host(host: HostOperation) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|operation| operation.host() == host)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_math_registry_has_unique_typed_pure_operations() {
        let mut names = std::collections::BTreeSet::new();
        let mut hosts = std::collections::BTreeSet::new();
        for &operation in NativeMath::ALL {
            assert!(names.insert(operation.name()));
            assert!(hosts.insert(operation.host()));
            assert_eq!(NativeMath::from_name(operation.name()), Some(operation));
            assert_eq!(NativeMath::from_host(operation.host()), Some(operation));
            let host = operation.host();
            assert_eq!(HostOperation::from_name(host.name()), Some(host));
            assert_eq!(host.minimum_abi(), crate::abi::NATIVE_MATH_VERSION);
            assert_eq!(host.capability(), None);
            assert!(!host.returns_string());
            assert!(!host.needs_result_globals());
            let (params, result) = host.scalar_signature();
            assert_eq!(params, vec![crate::HostScalar::F64; operation.arity()]);
            assert_eq!(result, &[crate::HostScalar::F64]);
        }
        assert_eq!(names.len(), 20);
        assert_eq!(NativeMath::from_name("random"), None);
        assert_eq!(NativeMath::from_host(HostOperation::FloatPower), None);
    }
}
