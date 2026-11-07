#!/usr/bin/env python3
"""Generate the table of CSS property names the published specifications
define (`crates/varta-common/src/view/css/known.rs`).

CSS Syntax 3 §5.4.4 and CSS 2 §4.2 drop a declaration whose property the
user agent does not know; the view engine refuses, instead of dropping, a
property a specification defines that it has not implemented yet, so it
needs the defined names. They are the W3C CSS property index (every
property of every CSS specification on w3.org/TR and the CSS WG editor's
drafts, `all-properties.en.json`) plus the properties of specifications
outside that index, listed below with their sources: the WHATWG Compat
Standard's `-webkit-` names, the legacy name aliases the CSS
specifications define, SVG 2's properties, MathML Core, Pointer Events,
Filter Effects 2 and CSS Mobile Text Size Adjustment.

Usage: generate-css-known-properties.py <all-properties.en.json>
(fetched from https://www.w3.org/Style/CSS/all-properties.en.json).
"""
import json
import sys
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "crates/varta-common/src/view/css/known.rs"

SUPPLEMENT = {
    # WHATWG Compat Standard §3.4.1 (legacy name aliases), §3.4.2, §3.4.4,
    # §3.4.6.
    "Compat Standard": [
        "-webkit-align-content", "-webkit-align-items", "-webkit-align-self",
        "-webkit-animation", "-webkit-animation-delay", "-webkit-animation-direction",
        "-webkit-animation-duration", "-webkit-animation-fill-mode",
        "-webkit-animation-iteration-count", "-webkit-animation-name",
        "-webkit-animation-play-state", "-webkit-animation-timing-function",
        "-webkit-backface-visibility", "-webkit-background-clip",
        "-webkit-background-origin", "-webkit-background-size",
        "-webkit-border-bottom-left-radius", "-webkit-border-bottom-right-radius",
        "-webkit-border-radius", "-webkit-border-top-left-radius",
        "-webkit-border-top-right-radius", "-webkit-box-shadow", "-webkit-box-sizing",
        "-webkit-flex", "-webkit-flex-basis", "-webkit-flex-direction", "-webkit-flex-flow",
        "-webkit-flex-grow", "-webkit-flex-shrink", "-webkit-flex-wrap", "-webkit-filter",
        "-webkit-justify-content", "-webkit-mask", "-webkit-mask-box-image",
        "-webkit-mask-box-image-outset", "-webkit-mask-box-image-repeat",
        "-webkit-mask-box-image-slice", "-webkit-mask-box-image-source",
        "-webkit-mask-box-image-width", "-webkit-mask-clip", "-webkit-mask-composite",
        "-webkit-mask-image", "-webkit-mask-origin", "-webkit-mask-position",
        "-webkit-mask-repeat", "-webkit-mask-size", "-webkit-order", "-webkit-perspective",
        "-webkit-perspective-origin", "-webkit-transform", "-webkit-transform-origin",
        "-webkit-transform-style", "-webkit-transition", "-webkit-transition-delay",
        "-webkit-transition-duration", "-webkit-transition-property",
        "-webkit-transition-timing-function", "-webkit-text-size-adjust",
        "-webkit-box-align", "-webkit-box-flex", "-webkit-box-ordinal-group",
        "-webkit-box-orient", "-webkit-box-pack", "-webkit-text-fill-color",
        "-webkit-text-stroke", "-webkit-text-stroke-color", "-webkit-text-stroke-width",
    ],
    # Legacy name aliases of the CSS specifications: Text 3 §5.5
    # (`word-wrap`), Box Alignment 3 §8.3 (`grid-*gap`), UI 4 §7.1
    # (`-webkit-appearance`) and §6.1 (`-webkit-user-select`).
    "CSS legacy name aliases": [
        "word-wrap", "grid-gap", "grid-row-gap", "grid-column-gap",
        "-webkit-appearance", "-webkit-user-select",
    ],
    # SVG 2 Appendix "Property Index" and the geometry properties of §9.
    "SVG 2": [
        "alignment-baseline", "baseline-shift", "clip-rule", "color-interpolation",
        "color-interpolation-filters", "color-rendering", "cx", "cy", "d",
        "dominant-baseline", "fill", "fill-opacity", "fill-rule", "flood-color",
        "flood-opacity", "glyph-orientation-vertical", "lighting-color", "marker",
        "marker-end", "marker-mid", "marker-start", "paint-order", "r", "rx", "ry",
        "shape-rendering", "stop-color", "stop-opacity", "stroke", "stroke-dasharray",
        "stroke-dashoffset", "stroke-linecap", "stroke-linejoin", "stroke-miterlimit",
        "stroke-opacity", "stroke-width", "text-anchor", "text-rendering",
        "vector-effect", "x", "y",
    ],
    "MathML Core": ["math-depth", "math-shift", "math-style"],
    "Pointer Events": ["touch-action"],
    "Filter Effects 2": ["backdrop-filter"],
    "CSS Mobile Text Size Adjustment": ["text-size-adjust"],
}


def main() -> None:
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    entries = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
    names = {entry["property"] for entry in entries if entry["property"] != "--*"}
    for extra in SUPPLEMENT.values():
        names.update(extra)
    for name in names:
        assert name == name.lower() and name.replace("-", "").isalnum(), name
    lines = [
        "//! CSS property names the published specifications define, generated by",
        "//! `scripts/generate-css-known-properties.py` from the W3C CSS property",
        "//! index (`all-properties.en.json`) and the supplementary specifications",
        "//! listed in that script. Do not edit.",
        "",
        "/// Every defined property name, ASCII lowercase, sorted.",
        "pub const KNOWN_PROPERTIES: &[&str] = &[",
    ]
    lines += [f'    "{name}",' for name in sorted(names)]
    lines += ["];", ""]
    OUT.write_text("\n".join(lines), encoding="utf-8")
    print(f"{len(names)} names -> {OUT}")


if __name__ == "__main__":
    main()
