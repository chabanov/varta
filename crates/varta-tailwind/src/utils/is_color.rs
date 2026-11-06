//! Port of Tailwind CSS v4.3.5 `utils/is-color.ts` (Tailwind Labs, MIT):
//! recognise CSS colour values syntactically.

/// CSS named colours, colour keywords and system colours, lowercase and
/// sorted by byte order for binary search.
const NAMED_COLORS: &[&str] = &[
    "accentcolor",
    "accentcolortext",
    "activetext",
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "buttonborder",
    "buttonface",
    "buttontext",
    "cadetblue",
    "canvas",
    "canvastext",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "currentcolor",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "field",
    "fieldtext",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "graytext",
    "green",
    "greenyellow",
    "grey",
    "highlight",
    "highlighttext",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "linktext",
    "magenta",
    "mark",
    "marktext",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "selecteditem",
    "selecteditemtext",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "transparent",
    "turquoise",
    "violet",
    "visitedtext",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

/// Colour function prefixes, matched ASCII case-insensitively (Tailwind's
/// `/^(rgba?|hsla?|hwb|color|(ok)?(lab|lch)|light-dark|color-mix|--alpha)\(/i`).
const COLOR_FUNCTION_PREFIXES: &[&str] = &[
    "rgb(",
    "rgba(",
    "hsl(",
    "hsla(",
    "hwb(",
    "color(",
    "lab(",
    "lch(",
    "oklab(",
    "oklch(",
    "light-dark(",
    "color-mix(",
    "--alpha(",
];

/// Whether `value` starts with `prefix`, ignoring ASCII case.
pub(crate) fn starts_with_ignore_ascii_case(value: &str, prefix: &str) -> bool {
    value
        .as_bytes()
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
}

/// Whether `value` is a colour: a hex colour (`#…`), a colour function or a
/// named colour.
#[must_use]
pub fn is_color(value: &str) -> bool {
    value.starts_with('#')
        || COLOR_FUNCTION_PREFIXES
            .iter()
            .any(|prefix| starts_with_ignore_ascii_case(value, prefix))
        || is_named_color(value)
}

/// Whether `value` is a named colour, colour keyword or system colour, in any
/// case.
#[must_use]
pub fn is_named_color(value: &str) -> bool {
    NAMED_COLORS
        .binary_search(&value.to_lowercase().as_str())
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_colors_are_sorted_and_unique() {
        assert!(NAMED_COLORS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn recognises_colors() {
        for value in [
            "#000",
            "#zzz",
            "rgb(0 0 0)",
            "RGBA(0,0,0,0)",
            "oklch(0.5 0.1 20)",
            "light-dark(white, black)",
            "--alpha(var(--a) / 50%)",
            "Red",
            "CurrentColor",
            "transparent",
            "canvastext",
            // The Kelvin sign lowercases to `k`, as in JavaScript.
            "blac\u{212A}",
        ] {
            assert!(is_color(value), "{value}");
        }
    }

    #[test]
    fn rejects_non_colors() {
        for value in [
            "",
            "1px",
            "var(--a)",
            "rgb",
            "redd",
            "color-mixx(a)",
            "okhsl(1 2 3)",
        ] {
            assert!(!is_color(value), "{value}");
        }
        assert!(!is_named_color("#000"));
        assert!(is_named_color("ALICEBLUE"));
    }
}
