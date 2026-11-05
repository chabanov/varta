//! CSS transitions of the native view (CSS Transitions 1, as Chrome 156 runs
//! them): `transition-property`, `-duration`, `-timing-function` and `-delay`
//! over the view subset's paint-only animatable properties.
//!
//! The view keeps an animation clock the host sets ([`crate::View::set_animation_time`]),
//! in milliseconds and never moving back. After every style change (a commit,
//! an input, a pointer or environment change) [`Animations::restyle`] compares
//! each displayed element's static values with the values before the change:
//! the running transition's current value, otherwise the last static value.
//! It starts, keeps, reverses or cancels transitions by the specification's
//! rules, with Chrome's reversing shortening. Reads for layout and paint then
//! see the current value of every running transition
//! ([`Animations::overlay`]); snapshots keep the static values. No transition
//! starts for an element's first style or across `display: none`, and a
//! finished transition leaves the static value.
use crate::{style::Environment, tree::Tree};
use std::collections::BTreeMap;
use varta_common::view::{CssProperty, ROOT_NODE};

/// The animatable properties the native view transitions, which `all`
/// covers. `transform`, also admitted in `transition-property`, has no
/// native value to animate.
const PROPERTIES: [CssProperty; 3] = [
    CssProperty::Color,
    CssProperty::BackgroundColor,
    CssProperty::Opacity,
];
const ALL: &str = "all";
const NONE: &str = "none";
const MILLISECONDS: &str = "ms";
const SECONDS: &str = "s";
const MILLISECONDS_PER_SECOND: f64 = 1000.0;
/// `background-color`'s initial value and `opacity`'s.
const INITIAL_BACKGROUND: Value = Value::Color([0.0; 4]);
const INITIAL_OPACITY: Value = Value::Number(1.0);
const CURRENT_COLOR: &str = "currentcolor";
const CHANNEL_MAX: f64 = 255.0;
/// Chrome's cubic Bézier solver tolerance (`gfx::CubicBezier`).
const BEZIER_EPSILON: f64 = 1e-7;
const NEWTON_ITERATIONS: usize = 8;
const BISECTION_ITERATIONS: usize = 64;
/// Decimal places of an animated alpha or opacity read by paint.
const FRACTION_DIGITS: usize = 6;

/// An animatable value: straight sRGB color channels and alpha in `0..=1`,
/// or a number.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Value {
    Color([f64; 4]),
    Number(f64),
}

impl Value {
    fn parse(property: CssProperty, value: &str) -> Option<Self> {
        match property {
            CssProperty::Opacity => value
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .map(|number| Self::Number(number.clamp(0.0, 1.0))),
            _ => {
                let color = crate::paint::parse_color(value)?;
                Some(Self::Color([
                    f64::from(color.red()),
                    f64::from(color.green()),
                    f64::from(color.blue()),
                    f64::from(color.alpha()),
                ]))
            }
        }
    }

    /// Interpolate at `progress`: numbers linearly, colors in sRGB with
    /// premultiplied alpha (CSS Color 4 for legacy colors), as Chrome does.
    fn interpolate(self, end: Self, progress: f64) -> Self {
        let mix = |a: f64, b: f64| a + (b - a) * progress;
        match (self, end) {
            (Self::Number(a), Self::Number(b)) => Self::Number(mix(a, b)),
            (Self::Color(a), Self::Color(b)) => {
                let alpha = mix(a[3], b[3]).clamp(0.0, 1.0);
                let channel = |i: usize| {
                    if alpha == 0.0 {
                        0.0
                    } else {
                        (mix(a[i] * a[3], b[i] * b[3]) / alpha).clamp(0.0, 1.0)
                    }
                };
                Self::Color([channel(0), channel(1), channel(2), alpha])
            }
            _ => end,
        }
    }

    /// The value as paint reads it.
    fn css(self) -> String {
        match self {
            Self::Number(number) => format!("{number:.FRACTION_DIGITS$}"),
            Self::Color([r, g, b, a]) => {
                let channel = |value: f64| (value * CHANNEL_MAX).round() as u8;
                format!(
                    "rgba({}, {}, {}, {a:.FRACTION_DIGITS$})",
                    channel(r),
                    channel(g),
                    channel(b)
                )
            }
        }
    }

    fn same_kind(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Number(_), Self::Number(_)) | (Self::Color(_), Self::Color(_))
        )
    }
}

/// A `transition-timing-function` keyword as its cubic Bézier.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Timing {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl Timing {
    const EASE: Self = Self::new(0.25, 0.1, 0.25, 1.0);

    const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    fn parse(keyword: &str) -> Option<Self> {
        Some(match keyword {
            "linear" => Self::new(0.0, 0.0, 1.0, 1.0),
            "ease" => Self::EASE,
            "ease-in" => Self::new(0.42, 0.0, 1.0, 1.0),
            "ease-out" => Self::new(0.0, 0.0, 0.58, 1.0),
            "ease-in-out" => Self::new(0.42, 0.0, 0.58, 1.0),
            _ => return None,
        })
    }

    /// The output progress at input progress `x` in `0..=1`.
    fn output(self, x: f64) -> f64 {
        if x <= 0.0 || x >= 1.0 {
            return x.clamp(0.0, 1.0);
        }
        // A curve on the diagonal (`linear`) is the identity.
        if self.x1 == self.y1 && self.x2 == self.y2 {
            return x;
        }
        let curve = |a: f64, b: f64, t: f64| {
            // B(t) for control points 0, a, b, 1.
            3.0 * a * t * (1.0 - t).powi(2) + 3.0 * b * t * t * (1.0 - t) + t.powi(3)
        };
        let slope = |a: f64, b: f64, t: f64| {
            3.0 * a * (1.0 - t).powi(2) + 6.0 * (b - a) * t * (1.0 - t) + 3.0 * (1.0 - b) * t * t
        };
        let mut t = x;
        for _ in 0..NEWTON_ITERATIONS {
            let error = curve(self.x1, self.x2, t) - x;
            if error.abs() < BEZIER_EPSILON {
                return curve(self.y1, self.y2, t);
            }
            let derivative = slope(self.x1, self.x2, t);
            if derivative.abs() < BEZIER_EPSILON {
                break;
            }
            t -= error / derivative;
        }
        let (mut low, mut high) = (0.0, 1.0);
        t = x;
        for _ in 0..BISECTION_ITERATIONS {
            let value = curve(self.x1, self.x2, t);
            if (value - x).abs() < BEZIER_EPSILON {
                break;
            }
            if value < x {
                low = t;
            } else {
                high = t;
            }
            t = (low + high) / 2.0;
        }
        curve(self.y1, self.y2, t)
    }
}

/// The matching `transition-*` values of one property.
#[derive(Debug, Clone, Copy)]
struct Declared {
    duration: f64,
    delay: f64,
    timing: Timing,
}

impl Declared {
    /// CSS Transitions 1: a transition needs a positive combined duration.
    fn combined_positive(self) -> bool {
        self.duration.max(0.0) + self.delay > 0.0
    }
}

/// One `<time>` in milliseconds.
fn time(value: &str) -> Option<f64> {
    let value = value.trim();
    let number = |text: &str| text.parse::<f64>().ok().filter(|n| n.is_finite());
    match value.strip_suffix(MILLISECONDS) {
        Some(milliseconds) => number(milliseconds),
        None => number(value.strip_suffix(SECONDS)?).map(|s| s * MILLISECONDS_PER_SECOND),
    }
}

/// The declared transition of `property` among an element's computed values:
/// the last `transition-property` entry naming it or `all` (its initial
/// value), with the duration,
/// delay and timing function at that entry's index (each list repeats).
fn declared(values: &BTreeMap<CssProperty, String>, property: CssProperty) -> Option<Declared> {
    // `transition-property` is `all` until declared.
    let names = values
        .get(&CssProperty::TransitionProperty)
        .map_or(ALL, String::as_str);
    let index = names
        .split(',')
        .map(str::trim)
        .enumerate()
        .filter(|(_, name)| *name == ALL || *name == property.name())
        .map(|(index, _)| index)
        .last()?;
    if names.trim() == NONE {
        return None;
    }
    let pick = |property: CssProperty, initial: &str| -> String {
        let list = values.get(&property).map_or(initial, String::as_str);
        let items: Vec<&str> = list.split(',').map(str::trim).collect();
        items[index % items.len()].to_owned()
    };
    Some(Declared {
        duration: time(&pick(CssProperty::TransitionDuration, "0s"))?,
        delay: time(&pick(CssProperty::TransitionDelay, "0s"))?,
        timing: Timing::parse(&pick(CssProperty::TransitionTimingFunction, "ease"))?,
    })
}

/// A running transition of one property of one element.
#[derive(Debug, Clone, PartialEq)]
struct Running {
    start: Value,
    end: Value,
    reversing_start: Value,
    shortening: f64,
    /// When the active interval begins (the style change plus the delay).
    begins: f64,
    duration: f64,
    timing: Timing,
}

impl Running {
    fn output(&self, now: f64) -> f64 {
        if now < self.begins {
            return 0.0;
        }
        if self.duration <= 0.0 {
            return 1.0;
        }
        self.timing
            .output(((now - self.begins) / self.duration).clamp(0.0, 1.0))
    }
    fn value(&self, now: f64) -> Value {
        if now < self.begins {
            return self.start;
        }
        self.start.interpolate(self.end, self.output(now))
    }
    fn finished(&self, now: f64) -> bool {
        now >= self.begins + self.duration.max(0.0)
    }
}

/// The animation clock, running transitions and every displayed element's
/// last static values.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Animations {
    now: f64,
    running: BTreeMap<(u32, CssProperty), Running>,
    last: BTreeMap<u32, [Option<Value>; PROPERTIES.len()]>,
}

impl Animations {
    /// Move the clock to `now` milliseconds (never back) and drop finished
    /// transitions. Returns whether a running value may have changed.
    pub(crate) fn advance(&mut self, now: f64) -> bool {
        if !now.is_finite() || now <= self.now {
            return false;
        }
        self.now = now;
        let had = !self.running.is_empty();
        self.running.retain(|_, running| !running.finished(now));
        had
    }

    /// Whether two trees paint the same animated values: neither runs a
    /// transition, or both run the same ones at the same clock.
    pub(crate) fn same_frame(&self, other: &Self) -> bool {
        (self.running.is_empty() && other.running.is_empty())
            || (self.now == other.now && self.running == other.running)
    }

    /// Whether a transition is running: the host should keep presenting frames.
    pub(crate) fn active(&self) -> bool {
        !self.running.is_empty()
    }

    /// The current value of every running transition of `id`, as paint reads it.
    pub(crate) fn overlay(&self, id: u32, values: &mut BTreeMap<CssProperty, String>) {
        for property in PROPERTIES {
            if let Some(value) = self.overlay_property(id, property) {
                values.insert(property, value);
            }
        }
    }

    /// The current value of one running transition, as paint reads it.
    pub(crate) fn overlay_property(&self, id: u32, property: CssProperty) -> Option<String> {
        self.running
            .get(&(id, property))
            .map(|running| running.value(self.now).css())
    }

    /// Apply one style change event at the current clock.
    pub(crate) fn restyle(tree: &mut Tree, env: &Environment) {
        let uses = tree.uses_transitions();
        if !uses && tree.animations.running.is_empty() {
            tree.animations.last.clear();
            return;
        }
        let order = tree.displayed_order(env);
        let mut last = BTreeMap::new();
        let mut running = BTreeMap::new();
        let now = tree.animations.now;
        for id in order {
            if id == ROOT_NODE || tree.element(id).is_none() {
                continue;
            }
            let values = tree.computed_static(id, env);
            let after = PROPERTIES.map(|property| after_value(tree, env, id, property, &values));
            let previous = tree.animations.last.get(&id);
            for (index, property) in PROPERTIES.into_iter().enumerate() {
                let key = (id, property);
                let current = tree.animations.running.get(&key);
                let before = current
                    .map(|running| running.value(now))
                    .or_else(|| previous.and_then(|values| values[index]));
                let decided = transition(
                    current,
                    before,
                    after[index],
                    declared(&values, property),
                    now,
                );
                if let Some(transition) = decided {
                    running.insert(key, transition);
                }
            }
            last.insert(id, after);
        }
        tree.animations.running = running;
        tree.animations.last = last;
    }
}

/// The value of `property` after a style change: `color` inherits (a control
/// without an author color has none to transition), the others take their
/// initial value when undeclared.
fn after_value(
    tree: &Tree,
    env: &Environment,
    id: u32,
    property: CssProperty,
    values: &BTreeMap<CssProperty, String>,
) -> Option<Value> {
    match property {
        CssProperty::Color => {
            let own = values.get(&property).filter(|v| *v != CURRENT_COLOR);
            match own {
                Some(value) => Value::parse(property, value),
                None => tree.path(id).into_iter().skip(1).find_map(|ancestor| {
                    tree.computed_static(ancestor, env)
                        .get(&property)
                        .filter(|v| *v != CURRENT_COLOR)
                        .and_then(|value| Value::parse(property, value))
                }),
            }
        }
        CssProperty::BackgroundColor => values
            .get(&property)
            .map_or(Some(INITIAL_BACKGROUND), |value| {
                Value::parse(property, value)
            }),
        _ => values
            .get(&property)
            .map_or(Some(INITIAL_OPACITY), |value| Value::parse(property, value)),
    }
}

/// CSS Transitions 1 §3 for one property: the transition that runs after the
/// change, if any.
fn transition(
    current: Option<&Running>,
    before: Option<Value>,
    after: Option<Value>,
    declared: Option<Declared>,
    now: f64,
) -> Option<Running> {
    let after = after?;
    let declared = declared.filter(|declared| declared.combined_positive())?;
    let start = |from: Value, reversing_start: Value, shortening: f64| {
        let duration = declared.duration.max(0.0) * shortening;
        let delay = if declared.delay < 0.0 {
            declared.delay * shortening
        } else {
            declared.delay
        };
        Running {
            start: from,
            end: after,
            reversing_start,
            shortening,
            begins: now + delay,
            duration,
            timing: declared.timing,
        }
    };
    match current {
        None => {
            let before = before?;
            (before != after && before.same_kind(after)).then(|| start(before, before, 1.0))
        }
        Some(running) if running.end == after => Some(running.clone()),
        Some(running) => {
            let value = running.value(now);
            if value == after || !value.same_kind(after) {
                None
            } else if running.reversing_start == after {
                let shortening = (running.output(now) * running.shortening
                    + (1.0 - running.shortening))
                    .abs()
                    .clamp(0.0, 1.0);
                Some(start(value, running.end, shortening))
            } else {
                Some(start(value, value, 1.0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_curves_match_their_end_points_and_known_values() {
        for keyword in ["linear", "ease", "ease-in", "ease-out", "ease-in-out"] {
            let timing = Timing::parse(keyword).unwrap();
            assert_eq!(timing.output(0.0), 0.0);
            assert_eq!(timing.output(1.0), 1.0);
        }
        let linear = Timing::parse("linear").unwrap();
        assert!((linear.output(0.3) - 0.3).abs() < 1e-6);
        // ease at 0.5: 0.8024033877399112 (the cubic-bezier(.25,.1,.25,1) value).
        assert!((Timing::EASE.output(0.5) - 0.802_403_387_739_911_2).abs() < 1e-6);
        assert!(Timing::parse("steps(2)").is_none());
    }

    #[test]
    fn times_lists_and_the_last_matching_entry_select_a_transition() {
        assert_eq!(time("120ms"), Some(120.0));
        assert_eq!(time("0.2s"), Some(200.0));
        assert_eq!(time("-1s"), Some(-1000.0));
        assert_eq!(time("2"), None);
        let values = |entries: &[(CssProperty, &str)]| {
            entries
                .iter()
                .map(|(p, v)| (*p, (*v).to_owned()))
                .collect::<BTreeMap<_, _>>()
        };
        let lists = values(&[
            (
                CssProperty::TransitionProperty,
                "background-color, color, all",
            ),
            (CssProperty::TransitionDuration, "100ms, 200ms"),
            (CssProperty::TransitionDelay, "5ms"),
        ]);
        // `all` at index 2 is the last match for both; durations repeat.
        let color = declared(&lists, CssProperty::Color).unwrap();
        assert_eq!((color.duration, color.delay), (100.0, 5.0));
        assert_eq!(color.timing, Timing::EASE);
        let named = values(&[
            (CssProperty::TransitionProperty, "background-color, color"),
            (CssProperty::TransitionDuration, "100ms, 200ms"),
        ]);
        assert_eq!(
            declared(&named, CssProperty::Color).unwrap().duration,
            200.0
        );
        assert!(declared(&named, CssProperty::Opacity).is_none());
        let none = values(&[(CssProperty::TransitionProperty, "none")]);
        assert!(declared(&none, CssProperty::Color).is_none());
        // Undeclared, the property list is `all`.
        let initial = values(&[(CssProperty::TransitionDuration, "1s")]);
        assert_eq!(
            declared(&initial, CssProperty::Opacity).unwrap().duration,
            1000.0
        );
        // Without a duration the combined duration is zero: no transition.
        let instant = values(&[(CssProperty::TransitionProperty, "all")]);
        assert!(
            !declared(&instant, CssProperty::Color)
                .unwrap()
                .combined_positive()
        );
    }

    #[test]
    fn colors_interpolate_with_premultiplied_alpha() {
        let red = Value::Color([1.0, 0.0, 0.0, 1.0]);
        let clear = Value::Color([0.0, 0.0, 1.0, 0.0]);
        // A transparent end keeps the opaque color's channels.
        assert_eq!(
            red.interpolate(clear, 0.5),
            Value::Color([1.0, 0.0, 0.0, 0.5])
        );
        assert_eq!(
            Value::Number(0.2).interpolate(Value::Number(1.0), 0.25),
            Value::Number(0.4)
        );
        assert_eq!(
            Value::Color([1.0, 0.5, 0.0, 0.5]).css(),
            "rgba(255, 128, 0, 0.500000)"
        );
    }

    #[test]
    fn a_reversed_transition_is_shortened_by_its_progress() {
        let declared = Declared {
            duration: 100.0,
            delay: 0.0,
            timing: Timing::parse("linear").unwrap(),
        };
        let a = Value::Number(0.0);
        let b = Value::Number(1.0);
        let forward = transition(None, Some(a), Some(b), Some(declared), 0.0).unwrap();
        assert_eq!(forward.value(25.0), Value::Number(0.25));
        // Back to the start value at 25%: a quarter of the duration remains.
        let back = transition(Some(&forward), None, Some(a), Some(declared), 25.0).unwrap();
        assert_eq!((back.duration, back.begins), (25.0, 25.0));
        assert_eq!(back.start, Value::Number(0.25));
        assert_eq!(back.reversing_start, b);
        // A third value restarts from the current value with the full duration.
        let other = transition(
            Some(&forward),
            None,
            Some(Value::Number(0.5)),
            Some(declared),
            25.0,
        )
        .unwrap();
        assert_eq!((other.duration, other.start), (100.0, Value::Number(0.25)));
        // The same end keeps it; no declaration cancels it.
        assert_eq!(
            transition(Some(&forward), None, Some(b), Some(declared), 50.0),
            Some(forward.clone())
        );
        assert_eq!(transition(Some(&forward), None, Some(a), None, 50.0), None);
        // A first style and an unchanged value start nothing.
        assert_eq!(transition(None, None, Some(b), Some(declared), 0.0), None);
        assert_eq!(
            transition(None, Some(b), Some(b), Some(declared), 0.0),
            None
        );
    }
}
