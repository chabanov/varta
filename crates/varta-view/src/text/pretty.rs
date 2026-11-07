//! Text 4 gives UAs freedom to improve wrapping quality. Search a bounded
//! number of widths to reduce a short final line without adding a line.
use parley::Layout;
const SEARCH_STEPS: usize = 8;
pub(super) const PASSES: usize = SEARCH_STEPS + 1;
const SHORT_LINE_FRACTION: f32 = 0.25;
fn last(layout: &Layout<u32>) -> f32 {
    layout.lines().last().map_or(0.0, |l| {
        l.metrics().advance - l.metrics().trailing_whitespace
    })
}
pub(super) fn improve(layout: &mut Layout<u32>, width: f32) {
    if layout.len() < 2 || last(layout) >= width * SHORT_LINE_FRACTION {
        return;
    }
    let lines = layout.len();
    let mut score = last(layout);
    let mut best = width;
    let (mut low, mut high) = (width * 0.5, width);
    for _ in 0..SEARCH_STEPS {
        let trial = (low + high) * 0.5;
        layout.break_all_lines(Some(trial));
        if layout.len() > lines {
            low = trial;
        } else {
            let next = last(layout);
            if layout.len() == lines && next > score {
                score = next;
                best = trial;
            }
            high = trial;
        }
    }
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(width);
    breaker.state_mut().set_line_max_advance(best);
    while breaker.break_next().is_some() {
        breaker.set_prior_line_width(width);
    }
    breaker.finish();
}
