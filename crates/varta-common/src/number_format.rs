//! Shared names and host operations for checked decimal Number formatting.
use crate::HostOperation;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumberFormat {
    Fixed,
    Exponential,
    Precision,
}
impl NumberFormat {
    pub const ALL: &[Self] = &[Self::Fixed, Self::Exponential, Self::Precision];
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fixed => "toFixed",
            Self::Exponential => "toExponential",
            Self::Precision => "toPrecision",
        }
    }
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|mode| mode.name() == name)
    }
    #[must_use]
    pub fn host(self) -> HostOperation {
        match self {
            Self::Fixed => HostOperation::NumberToFixed,
            Self::Exponential => HostOperation::NumberToExponential,
            Self::Precision => HostOperation::NumberToPrecision,
        }
    }
}
