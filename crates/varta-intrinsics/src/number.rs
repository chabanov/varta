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

/// Exact powers of ten up to the fast path's largest fraction scale.
static SCALES: [f64; 7] = [1.0, 10.0, 100.0, 1e3, 1e4, 1e5, 1e6];
/// Integers below this are exact in binary64.
const EXACT_INTEGERS: f64 = 9_007_199_254_740_992.0;
/// Decimals of at most fifteen significant digits convert to distinct
/// doubles (`DBL_DIG`), so one such decimal that converts to a value is its
/// shortest text.
const SIGNIFICANT: f64 = 1e15;
/// The longest fast-path text: a sign, sixteen digits, a point and six
/// fraction digits with their leading zero.
const FAST_BYTES: usize = 26;

/// The shortest round-trip text of a finite Number, exactly
/// [`canonical_display`]'s, with a fast path that needs no 128-bit
/// arithmetic (the cost of `ryu` in 32-bit Wasm): integers below 2^53, and
/// decimals of at most six fraction digits and fifteen significant digits,
/// checked exactly (the correctly rounded quotient of the decimal's integer
/// and its power of ten is the Number itself). Every other Number goes to
/// `ryu-js`.
pub struct Shortest {
    fast: [u8; FAST_BYTES],
    ryu: ryu_js::Buffer,
}
impl Default for Shortest {
    fn default() -> Self {
        Self::new()
    }
}
impl Shortest {
    #[must_use]
    pub fn new() -> Self {
        Self {
            fast: [0; FAST_BYTES],
            ryu: ryu_js::Buffer::new(),
        }
    }
    /// The ASCII text of `value`, which must be finite (`-0` is `"0"`).
    pub fn format_finite(&mut self, value: f64) -> &[u8] {
        if let Some((digits, scale)) = decimal(value) {
            return self.fixed(value.is_sign_negative() && digits != 0, digits, scale);
        }
        self.ryu.format_finite(value).as_bytes()
    }
    /// `digits / 10^scale` in fixed notation.
    fn fixed(&mut self, negative: bool, digits: u64, scale: usize) -> &[u8] {
        let mut text = [0u8; 20];
        let mut start = text.len();
        let mut rest = digits;
        loop {
            start -= 1;
            text[start] = b'0' + (rest % 10) as u8;
            rest /= 10;
            if rest == 0 {
                break;
            }
        }
        let digits = &text[start..];
        let mut len = 0;
        let mut push = |fast: &mut [u8; FAST_BYTES], bytes: &[u8]| {
            fast[len..len + bytes.len()].copy_from_slice(bytes);
            len += bytes.len();
        };
        if negative {
            push(&mut self.fast, b"-");
        }
        if scale == 0 {
            push(&mut self.fast, digits);
        } else if digits.len() <= scale {
            push(&mut self.fast, b"0.");
            for _ in digits.len()..scale {
                push(&mut self.fast, b"0");
            }
            push(&mut self.fast, digits);
        } else {
            let point = digits.len() - scale;
            push(&mut self.fast, &digits[..point]);
            push(&mut self.fast, b".");
            push(&mut self.fast, &digits[point..]);
        }

        &self.fast[..len]
    }
}
/// `|value|` as `digits / 10^scale` with the smallest scale, when the fast
/// path applies.
fn decimal(value: f64) -> Option<(u64, usize)> {
    let magnitude = libm::fabs(value);
    if libm::trunc(magnitude) == magnitude && magnitude < EXACT_INTEGERS {
        return Some((magnitude as u64, 0));
    }
    if magnitude >= SIGNIFICANT {
        return None;
    }
    for (scale, power) in SCALES.iter().enumerate().skip(1) {
        let digits = libm::round(magnitude * power);
        if digits < SIGNIFICANT && digits / power == magnitude {
            return Some((digits as u64, scale));
        }
    }
    None
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
    fn check(value: f64) {
        if !value.is_finite() {
            return;
        }
        let expected = canonical_display(value);
        assert_eq!(
            core::str::from_utf8(Shortest::new().format_finite(value)).unwrap(),
            expected,
            "{value:e} ({:#x})",
            value.to_bits()
        );
    }
    #[test]
    fn the_fast_path_writes_exactly_the_shortest_text() {
        for value in [
            0.0,
            -0.0,
            1.0,
            -1.0,
            1.5,
            -2.5,
            0.5,
            0.25,
            0.125,
            0.1,
            0.2,
            0.3,
            0.1 + 0.2,
            1e-6,
            -1e-6,
            1e-7,
            0.000001,
            0.0000015,
            123456.789,
            999999999999999.0,
            1e15,
            1e15 + 0.5,
            9007199254740991.0,
            9007199254740992.0,
            9007199254740993.0,
            1e21,
            1e22,
            5e-324,
            f64::MAX,
            f64::MIN_POSITIVE,
            4.35,
            0.07,
            2.675,
            1.005,
            100.0,
            1e20,
            123e18,
        ] {
            check(value);
        }
        let mut state = 0x853c_49e6_748f_ea9bu64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..200_000 {
            let bits = next();
            // Arbitrary doubles, integers of every size, and decimals of
            // up to eight fraction digits.
            check(f64::from_bits(bits));
            check((bits >> (bits % 64)) as i64 as f64);
            let scale = SCALES.len() + 2;
            let digits = (next() >> (next() % 64)) as f64;
            check(digits / libm::pow(10.0, (bits % scale as u64) as f64));
            check(-(digits % 1e9) / libm::pow(10.0, (bits % scale as u64) as f64));
        }
    }
}
