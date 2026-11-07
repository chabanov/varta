//! A virtual-time CSS Animations timeline. No host clock or worker is owned here.
use super::*;
use varta_common::view::{
    animation,
    syntax::{self, Token},
};
#[derive(Debug, Clone, PartialEq)]
struct Point {
    offset: f64,
    value: Value,
    timing: Timing,
}
#[derive(Debug, Clone, PartialEq)]
struct Binding {
    name: String,
    elapsed: f64,
    duration: f64,
    delay: f64,
    iterations: f64,
    direction: String,
    fill: String,
    paused: bool,
    values: BTreeMap<CssProperty, Vec<Point>>,
    unsupported: std::collections::BTreeSet<CssProperty>,
}
impl Binding {
    fn active(&self) -> bool {
        !self.paused && self.elapsed < self.delay + self.total()
    }
    fn total(&self) -> f64 {
        if self.duration == 0.0 {
            0.0
        } else {
            self.duration * self.iterations
        }
    }
    fn progress(&self) -> Option<f64> {
        let elapsed = self.elapsed - self.delay;
        let (iteration, mut fraction) = if elapsed < 0.0 {
            if !matches!(self.fill.as_str(), "backwards" | "both") {
                return None;
            }
            (0.0, 0.0)
        } else if elapsed >= self.total() {
            if !matches!(self.fill.as_str(), "forwards" | "both") {
                return None;
            }
            if self.iterations == 0.0 {
                (0.0, 0.0)
            } else {
                (
                    (self.iterations.ceil() - 1.0).max(0.0),
                    if self.iterations.fract() == 0.0 {
                        1.0
                    } else {
                        self.iterations.fract()
                    },
                )
            }
        } else {
            let position = elapsed / self.duration;
            (position.floor(), position.fract())
        };
        let reverse = matches!(self.direction.as_str(), "reverse" | "alternate-reverse")
            ^ (matches!(self.direction.as_str(), "alternate" | "alternate-reverse")
                && iteration.rem_euclid(2.0) == 1.0);
        if reverse {
            fraction = 1.0 - fraction;
        }
        Some(fraction)
    }
    fn value(&self, p: CssProperty) -> Option<Value> {
        let fraction = self.progress()?;
        let points = self.values.get(&p)?;
        let upper = points.partition_point(|v| v.offset <= fraction);
        if upper == 0 {
            return Some(points[0].value.clone());
        }
        if upper == points.len() {
            return Some(points[upper - 1].value.clone());
        }
        let (a, b) = (&points[upper - 1], &points[upper]);
        let progress = a
            .timing
            .output((fraction - a.offset) / (b.offset - a.offset));
        Some(if a.value.same_kind(&b.value) {
            a.value.interpolate(&b.value, progress)
        } else if progress < 0.5 {
            a.value.clone()
        } else {
            b.value.clone()
        })
    }
}
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Timeline {
    bindings: BTreeMap<u32, Vec<Binding>>,
}
fn names(values: &BTreeMap<CssProperty, String>) -> Vec<String> {
    animation::list(
        values
            .get(&CssProperty::AnimationName)
            .map_or("none", String::as_str),
    )
    .unwrap_or_default()
    .iter()
    .filter_map(|name| {
        let tokens = syntax::tokenize(name);
        if let [
            syntax::Spanned {
                token: Token::Ident(name),
                ..
            },
        ] = tokens.as_slice()
        {
            Some(name.clone())
        } else {
            None
        }
    })
    .collect()
}
fn entry(values: &BTreeMap<CssProperty, String>, slot: usize, index: usize) -> &str {
    // Owned list slices are short-lived, but their text borrows the source map.
    let source = values
        .get(&animation::PROPERTIES[slot])
        .map_or(animation::INITIAL[slot], String::as_str);
    let parts = animation::list(source).unwrap_or_else(|| vec![animation::INITIAL[slot]]);
    parts[index % parts.len()]
}
impl Timeline {
    pub(super) fn advance(&mut self, delta: f64) -> bool {
        let mut had = false;
        for bindings in self.bindings.values_mut() {
            for b in bindings {
                had |= b.active();
                if !b.paused {
                    b.elapsed += delta;
                }
            }
        }
        had
    }
    pub(super) fn active(&self) -> bool {
        self.bindings.values().flatten().any(Binding::active)
    }
    pub(super) fn value(&self, id: u32, p: CssProperty) -> Option<Result<Value, ()>> {
        self.bindings.get(&id)?.iter().rev().find_map(|b| {
            if b.unsupported.contains(&p) {
                Some(Err(()))
            } else {
                b.value(p).map(Ok)
            }
        })
    }
    pub(super) fn restyle(tree: &mut Tree, env: &Environment) {
        if tree
            .styles
            .sheets()
            .values()
            .all(|s| s.keyframes.is_empty())
        {
            tree.animations.keyframes.bindings.clear();
            return;
        }
        let mut bindings = BTreeMap::new();
        let units = match Units::new(env) {
            Some(u) => u,
            None => return,
        };
        for id in tree.displayed_order(env) {
            if tree.element(id).is_none() {
                continue;
            }
            let values = tree.computed_static(id, env);
            let mut own = Vec::new();
            for (index, name) in names(&values).into_iter().enumerate() {
                if name == "none" {
                    continue;
                }
                let Some(rule) = tree.styles.keyframes(&name, &env.style()) else {
                    continue;
                };
                let timing = Timing::parse(entry(&values, 2, index)).unwrap_or(Timing::EASE);
                let mut property_points =
                    BTreeMap::<CssProperty, BTreeMap<u64, (Value, Timing)>>::new();
                let cascade = tree.styles.cascade(tree, id, &env.style().media);
                let mut unsupported = std::collections::BTreeSet::new();
                let mut merged = BTreeMap::<u64, BTreeMap<CssProperty, String>>::new();
                for frame in &rule.frames {
                    merged
                        .entry(frame.offset.to_bits())
                        .or_default()
                        .extend(frame.declarations.iter().cloned());
                }
                for (offset, declarations) in merged {
                    let easing = declarations
                        .iter()
                        .rev()
                        .find(|(p, _)| **p == CssProperty::AnimationTimingFunction)
                        .and_then(|(_, v)| Timing::parse(v))
                        .unwrap_or(timing);
                    for (p, v) in &declarations {
                        if !PROPERTIES.contains(p)
                            || cascade
                                .important
                                .iter()
                                .any(|d| d.name.property() == Some(*p))
                        {
                            continue;
                        }
                        let context = varta_common::view::css::Context {
                            font: tree.styles.font_context(tree, id, &env.style()).ok(),
                            ..env.style().context
                        };
                        let lowered = varta_common::view::css::lower(*p, v, &context);
                        let Some(v) = lowered.as_deref() else {
                            continue;
                        };
                        let v = if v == CURRENT_COLOR {
                            after_value(tree, env, id, CssProperty::Color, &values)
                        } else if matches!(v, "initial" | "inherit" | "unset") {
                            let mut base = values.clone();
                            base.remove(p);
                            if v == "inherit" || v == "unset" && matches!(p, CssProperty::Color) {
                                tree.nodes[&id].parent.and_then(|parent| {
                                    after_value(
                                        tree,
                                        env,
                                        parent,
                                        *p,
                                        &tree.computed_static(parent, env),
                                    )
                                })
                            } else {
                                after_value(tree, env, id, *p, &base)
                            }
                        } else {
                            Value::parse(*p, v, units)
                        };
                        if v.is_none() && *p == CssProperty::BoxShadow {
                            // A valid but unresolved animated color expression
                            // must not disappear from the native keyframe rule.
                            unsupported.insert(*p);
                        }
                        if let Some(v) = v {
                            property_points
                                .entry(*p)
                                .or_default()
                                .insert(offset, (v, easing));
                        }
                    }
                }
                let mut points = BTreeMap::new();
                for (p, mut list) in property_points {
                    if let Some(base) = after_value(tree, env, id, p, &values) {
                        list.entry(0.0_f64.to_bits())
                            .or_insert((base.clone(), timing));
                        list.entry(1.0_f64.to_bits()).or_insert((base, timing));
                    }
                    let values: Vec<_> = list.values().map(|(v, _)| v).collect();
                    if values.windows(2).any(|pair| {
                        matches!(pair[0], Value::Transform(_)) && !pair[0].same_kind(pair[1])
                    }) {
                        unsupported.insert(p);
                    }
                    points.insert(
                        p,
                        list.into_iter()
                            .map(|(offset, (value, timing))| Point {
                                offset: f64::from_bits(offset),
                                value,
                                timing,
                            })
                            .collect(),
                    );
                }
                let old = tree
                    .animations
                    .keyframes
                    .bindings
                    .get(&id)
                    .and_then(|bs| bs.iter().find(|b| b.name == name));
                own.push(Binding {
                    name,
                    elapsed: old.map_or(0.0, |b| b.elapsed),
                    duration: time(entry(&values, 1, index)).unwrap_or(0.0).max(0.0),
                    delay: time(entry(&values, 3, index)).unwrap_or(0.0),
                    iterations: if entry(&values, 4, index) == "infinite" {
                        f64::INFINITY
                    } else {
                        entry(&values, 4, index).parse().unwrap_or(1.0)
                    },
                    direction: entry(&values, 5, index).into(),
                    fill: entry(&values, 6, index).into(),
                    paused: entry(&values, 7, index) == "paused",
                    values: points,
                    unsupported,
                });
            }
            if !own.is_empty() {
                bindings.insert(id, own);
            }
        }
        tree.animations.keyframes = Timeline { bindings };
    }
}
