//! One integration harness preserves per-file modules and formatting coverage.
mod accessibility;
mod accessibility_reveal;
mod accessibility_scroll;
mod accessibility_text;
mod border_colors;
mod cascade_work;
mod checkables;
mod clipboard;
mod css_engine;
mod css_pseudo;
mod css_s4;
mod css_s5;
mod css_shadows;
mod dialog;
mod expansion_work;
mod fallback;
mod fields;
mod glyph_raster;
mod headless;
mod inline;
mod inline_border;
mod inline_edges;
mod inline_opacity;
mod inline_outline;
mod inline_round;
mod inline_spacing;
mod intrinsic_sizes;
mod lengths;
mod naming_work;
mod opacity;
mod paint_order;
mod paint_snap;
mod paragraphs;
mod pointer;
mod renderer;
mod renderer_inputs;
mod retained_text;
mod rounded;
mod scrollbars;
mod selects;
mod snapshot_export;
mod structure;
mod style_work;
mod text;
mod text_geometry;
mod transforms;
mod transitions;
mod tree_depth;
mod underline;
mod variables;

#[test]
fn every_case_file_is_registered() {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut files = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|extension| extension == "rs") {
            let name = path.file_stem().unwrap().to_str().unwrap();
            if name != "integration" {
                files.insert(name.to_owned());
            }
        }
    }
    // Keep unconditional, ordinary module declarations: rustfmt follows those
    // files too, whereas declarations hidden in a macro lose child formatting.
    let declared = include_str!("integration.rs")
        .lines()
        .filter_map(|line| {
            line.strip_prefix("mod ")
                .and_then(|name| name.strip_suffix(';'))
                .map(str::to_owned)
        })
        .collect();
    assert_eq!(
        files, declared,
        "declare every integration case file as a module"
    );
}

#[path = "css_typography_controls.rs"]
mod css_typography_controls;

#[path = "css_clipping.rs"]
mod css_clipping;

#[path = "css_sticky.rs"]
mod css_sticky;

#[path = "css_line_clamp.rs"]
mod css_line_clamp;

#[path = "css_atomic_inline.rs"]
mod css_atomic_inline;

#[path = "css_gradients.rs"]
mod css_gradients;

#[path = "css_backdrop.rs"]
mod css_backdrop;

#[path = "css_animations.rs"]
mod css_animations;

mod css_shadow_motion;
