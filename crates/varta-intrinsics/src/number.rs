//! Canonical Number text (`Number::toString(x)`, exactly JavaScript's `String(x)`).
use alloc::{borrow::ToOwned, string::String};

/// ECMAScript `Number::toString(x)` in radix 10: `-0` is `"0"`, exponents
/// follow ECMAScript (`1e+21`), through the pinned `ryu-js`.
#[must_use]
pub fn canonical_display(value: f64) -> String {
    if value == 0.0 {
        "0".into()
    } else {
        ryu_js::Buffer::new().format(value).to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_text_matches_javascript() {
        for (value, text) in [
            (0.0, "0"),
            (-0.0, "0"),
            (1e21, "1e+21"),
            (1e-7, "1e-7"),
            (0.1, "0.1"),
            (f64::NAN, "NaN"),
            (f64::NEG_INFINITY, "-Infinity"),
            (123.456, "123.456"),
        ] {
            assert_eq!(canonical_display(value), text);
        }
    }
}
