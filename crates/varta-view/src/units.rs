//! Native CSS length conversion against explicit host reference metrics.
use crate::Environment;
use taffy::prelude::{
    Dimension, FromLength, FromPercent, LengthPercentage, LengthPercentageAuto, auto, length,
    percent,
};

/// A Taffy length type that holds a layout-time `calc()` handle
/// ([`crate::calc`]).
pub(crate) trait FromCalc {
    fn from_calc(handle: *const ()) -> Self;
}
impl FromCalc for LengthPercentage {
    fn from_calc(handle: *const ()) -> Self {
        Self::calc(handle)
    }
}
impl FromCalc for LengthPercentageAuto {
    fn from_calc(handle: *const ()) -> Self {
        Self::calc(handle)
    }
}
impl FromCalc for Dimension {
    fn from_calc(handle: *const ()) -> Self {
        Self::calc(handle)
    }
}

pub(crate) const MAX_LENGTH: f32 = 1_000_000.0;

#[derive(Clone, Copy)]
pub(crate) struct Units {
    root_font_size: f32,
}

impl Units {
    pub(crate) fn new(environment: &Environment) -> Option<Self> {
        let size = environment.root_font_size;
        (size.is_finite() && (0.0..=f64::from(MAX_LENGTH)).contains(&size)).then_some(Self {
            root_font_size: size as f32,
        })
    }

    pub(crate) fn pixels(self, value: &str, signed: bool) -> Option<f32> {
        let (number, scale) = if value == "0" {
            ("0", 1.0)
        } else if let Some(number) = value.strip_suffix("px") {
            (number, 1.0)
        } else {
            (value.strip_suffix("rem")?, self.root_font_size)
        };
        let number = number.parse::<f32>().ok()?;
        if !Self::bounded(number, signed) {
            return None;
        }
        let pixels = number * scale;
        Self::bounded(pixels, signed).then_some(pixels)
    }

    pub(crate) fn length<T: FromLength + FromPercent + FromCalc>(
        self,
        value: &str,
        signed: bool,
    ) -> Option<T> {
        if value.ends_with(')') {
            // A lowered layout-time `calc()` (CSS Values 4 §10.10).
            crate::calc::handle(value).map(T::from_calc)
        } else if let Some(number) = value.strip_suffix('%') {
            let number = number.parse::<f32>().ok()?;
            Self::bounded(number, signed).then(|| percent(number / 100.0))
        } else {
            self.pixels(value, signed).map(length)
        }
    }

    pub(crate) fn margin(self, value: &str) -> Option<LengthPercentageAuto> {
        if value == "auto" {
            Some(auto())
        } else {
            self.length(value, true)
        }
    }

    /// Grid track syntax contains only dimensions, numbers and the admitted
    /// functions. Normalize whole tokens, never a suffix inside an identifier.
    pub(crate) fn grid(self, value: &str) -> Option<String> {
        let mut normalized = String::with_capacity(value.len());
        let mut start = 0;
        for (at, ch) in value
            .char_indices()
            .chain(std::iter::once((value.len(), ' ')))
        {
            if ch.is_ascii_whitespace() || matches!(ch, '(' | ')' | ',') {
                let token = &value[start..at];
                if token == "0" || token.ends_with("rem") {
                    use std::fmt::Write;
                    write!(normalized, "{}px", self.pixels(token, false)?).ok()?;
                } else {
                    normalized.push_str(token);
                }
                if at < value.len() {
                    normalized.push(ch);
                }
                start = at + ch.len_utf8();
            }
        }
        Some(normalized)
    }

    fn bounded(value: f32, signed: bool) -> bool {
        value.is_finite() && value.abs() <= MAX_LENGTH && (signed || value >= 0.0)
    }
}
