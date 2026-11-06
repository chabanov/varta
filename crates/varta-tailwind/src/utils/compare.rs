//! Port of Tailwind CSS v4.3.5 `utils/compare.ts` (Tailwind Labs, MIT):
//! natural ordering of class names, numbers compared by value.

use std::cmp::Ordering;

use super::js_number::to_number;

/// The UTF-16 code unit range of ASCII decimal digits.
const ZERO: u16 = b'0' as u16;
const NINE: u16 = b'9' as u16;

fn is_digit(unit: Option<&u16>) -> bool {
    unit.is_some_and(|unit| (ZERO..=NINE).contains(unit))
}

/// Compare two strings alphanumerically: runs of digits at the same position
/// compare as numbers (`p-2` before `p-10`), everything else by UTF-16 code
/// unit as JavaScript does, and a prefix sorts first.
#[must_use]
pub fn compare(a: &str, z: &str) -> Ordering {
    let a: Vec<u16> = a.encode_utf16().collect();
    let z: Vec<u16> = z.encode_utf16().collect();
    let min_len = a.len().min(z.len());

    let mut i = 0;
    while i < min_len {
        let a_code = a[i];
        let z_code = z[i];

        // If both are numbers, compare them as numbers instead of strings.
        if is_digit(Some(&a_code)) && is_digit(Some(&z_code)) {
            let mut a_end = i + 1;
            while is_digit(a.get(a_end)) {
                a_end += 1;
            }
            let mut z_end = i + 1;
            while is_digit(z.get(z_end)) {
                z_end += 1;
            }

            let a_number = &a[i..a_end];
            let z_number = &z[i..z_end];

            // Digits are ASCII, so the slices convert losslessly.
            let diff = to_number(&String::from_utf16_lossy(a_number))
                - to_number(&String::from_utf16_lossy(z_number));
            // `NaN` (two overflowing numbers) and zero fall through, as in
            // JavaScript's truthiness test.
            if let Some(order) = diff.partial_cmp(&0.0).filter(|order| order.is_ne()) {
                return order;
            }

            // Same value, different spelling (`0123` vs `123`): compare as
            // strings.
            match a_number.cmp(z_number) {
                Ordering::Equal => {}
                order => return order,
            }

            // Continue with the next character, otherwise short strings
            // would appear after long ones (`bg-red-500/70`, `bg-red-500`).
            // Tailwind's loop resumes at `i + 1`, inside the number run.
            i += 1;
            continue;
        }

        if a_code != z_code {
            return a_code.cmp(&z_code);
        }
        i += 1;
    }

    // Equal up to the length of the shorter string: it comes first.
    a.len().cmp(&z.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use Ordering::{Equal, Greater, Less};

    #[test]
    fn compares_pairs() {
        for (a, z, expected) in [
            ("abc", "abc", Equal),
            ("abc", "abcd", Less),
            ("abcd", "abc", Greater),
            ("1", "1", Equal),
            ("1", "2", Less),
            ("2", "1", Greater),
            ("1", "10", Less),
            ("10", "1", Greater),
            ("75", "700", Less),
            ("700", "75", Greater),
            ("75", "770", Less),
            ("770", "75", Greater),
            ("0123", "123", Less),
        ] {
            assert_eq!(compare(a, z), expected, "{a} vs {z}");
        }
    }

    fn sorted(input: &[&str]) -> Vec<String> {
        // Reverse first so the result does not depend on the input order.
        let mut values: Vec<String> = input.iter().rev().map(|s| (*s).to_owned()).collect();
        values.sort_by(|a, z| compare(a, z));
        values
    }

    #[test]
    fn sorts_strings_with_numbers() {
        assert_eq!(
            sorted(&[
                "p-0", "p-0.5", "p-1", "p-1.5", "p-10", "p-12", "p-2", "p-20", "p-21"
            ]),
            [
                "p-0", "p-0.5", "p-1", "p-1.5", "p-2", "p-10", "p-12", "p-20", "p-21"
            ]
        );
    }

    #[test]
    fn sorts_strings_with_modifiers() {
        assert_eq!(
            sorted(&[
                "text-5xl",
                "text-6xl",
                "text-6xl/loose",
                "text-6xl/wide",
                "bg-red-500",
                "bg-red-500/50",
                "bg-red-500/70",
                "bg-red-500/60",
                "bg-red-50",
                "bg-red-50/50",
                "bg-red-50/70",
                "bg-red-50/60",
            ]),
            [
                "bg-red-50",
                "bg-red-50/50",
                "bg-red-50/60",
                "bg-red-50/70",
                "bg-red-500",
                "bg-red-500/50",
                "bg-red-500/60",
                "bg-red-500/70",
                "text-5xl",
                "text-6xl",
                "text-6xl/loose",
                "text-6xl/wide",
            ]
        );
    }

    #[test]
    fn sorts_strings_with_multiple_numbers() {
        assert_eq!(
            sorted(&[
                "foo-123-bar-456-baz-789",
                "foo-123-bar-456-baz-788",
                "foo-123-bar-456-baz-790",
                "foo-123-bar-455-baz-789",
                "foo-123-bar-456-baz-789",
                "foo-123-bar-457-baz-789",
                "foo-123-bar-456-baz-789",
                "foo-124-bar-456-baz-788",
                "foo-125-bar-456-baz-790",
                "foo-126-bar-455-baz-789",
                "foo-127-bar-456-baz-789",
                "foo-128-bar-457-baz-789",
                "foo-1-bar-2-baz-3",
                "foo-12-bar-34-baz-45",
                "foo-12-bar-34-baz-4",
                "foo-12-bar-34-baz-456",
            ]),
            [
                "foo-1-bar-2-baz-3",
                "foo-12-bar-34-baz-4",
                "foo-12-bar-34-baz-45",
                "foo-12-bar-34-baz-456",
                "foo-123-bar-455-baz-789",
                "foo-123-bar-456-baz-788",
                "foo-123-bar-456-baz-789",
                "foo-123-bar-456-baz-789",
                "foo-123-bar-456-baz-789",
                "foo-123-bar-456-baz-790",
                "foo-123-bar-457-baz-789",
                "foo-124-bar-456-baz-788",
                "foo-125-bar-456-baz-790",
                "foo-126-bar-455-baz-789",
                "foo-127-bar-456-baz-789",
                "foo-128-bar-457-baz-789",
            ]
        );
    }

    #[test]
    fn sort_is_stable_for_every_permutation() {
        // Heap's algorithm over all permutations.
        let mut input = vec![
            "duration-initial",
            "duration-75",
            "duration-150",
            "duration-700",
            "duration-1000",
        ];
        let expected = [
            "duration-75",
            "duration-150",
            "duration-700",
            "duration-1000",
            "duration-initial",
        ];
        let check = |permutation: &[&str]| {
            let mut sorted = permutation.to_vec();
            sorted.sort_by(|a, z| compare(a, z));
            assert_eq!(sorted, expected);
        };
        let mut stack = vec![0; input.len()];
        check(&input);
        let mut pos = 1;
        while pos < input.len() {
            if stack[pos] < pos {
                let k = if pos % 2 == 0 { 0 } else { stack[pos] };
                input.swap(k, pos);
                check(&input);
                stack[pos] += 1;
                pos = 1;
            } else {
                stack[pos] = 0;
                pos += 1;
            }
        }
    }

    #[test]
    fn compares_utf16_code_units() {
        // U+FF5E sorts after the surrogate pair of U+1F600 in UTF-16.
        assert_eq!(compare("\u{1F600}", "\u{FF5E}"), Less);
    }
}
