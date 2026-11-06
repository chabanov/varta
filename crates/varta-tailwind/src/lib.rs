#![forbid(unsafe_code)]
//! Build-time Tailwind CSS compiler (K8 T1).
//!
//! A Rust port of the CSS-first compiler of Tailwind CSS v4.3.5
//! (<https://github.com/tailwindlabs/tailwindcss>, MIT, Copyright (c)
//! Tailwind Labs, Inc.; the notice is in `LICENSE-TAILWIND`). Given a
//! project's Tailwind entry stylesheet and the class candidates extracted
//! from its module graph, it produces one stylesheet in canonical form (no
//! comments, flattened nesting, Media Queries 4 ranges as `min-`/`max-`
//! features, `@supports` resolved statically against Varta's stylesheet
//! grammar). The same inputs always give the same bytes.
//!
//! JavaScript configuration (`@plugin`, `@config`) is rejected.
pub mod ast;
pub mod candidate;
pub mod canonical;
pub mod compile;
pub mod css_functions;
pub mod css_parser;
pub mod design_system;
pub mod optimize;
pub mod printer;
pub mod property_order;
pub mod selector_parser;
pub mod theme;
pub mod utilities;
pub mod utils;
pub mod value_parser;
pub mod variants;
