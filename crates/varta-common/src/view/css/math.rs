//! Math functions of CSS Values and Units 4 §10 (`calc()`, `min()`,
//! `max()`, `clamp()`), over the value tokens of [`super::V`].
//!
//! Parsing follows the §10.1 grammar (`<calc-sum>`, `<calc-product>`,
//! `<calc-value>`, nested `( … )` and math functions, the constants of
//! §10.7.1), type checking follows §10.7 (each operand's type, `+`/`-` need
//! equal types, `*` adds and `/` subtracts exponents), and evaluation
//! follows §10.9–§10.12: a top-level NaN becomes 0 and infinities are
//! clamped to the target range. Every node is bounded by the value's token
//! limit ([`super::MAX_VALUE_TOKENS`]).
use super::V;

/// The math functions of §10 the engine admits; every other function name
/// (`round()`, `sin()`, …) is outside the subset.
pub const FUNCTIONS: &[&str] = &["calc", "min", "max", "clamp"];

/// A base type of §10.7 with its canonical unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Base {
    Length,
    Angle,
    Time,
    Percent,
}

/// A type of §10.7: the exponent of each base type (all zero is `<number>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Type {
    length: i8,
    angle: i8,
    time: i8,
    percent: i8,
}
impl Type {
    pub const NUMBER: Self = Self {
        length: 0,
        angle: 0,
        time: 0,
        percent: 0,
    };
    #[must_use]
    pub const fn of(base: Base) -> Self {
        let mut out = Self::NUMBER;
        match base {
            Base::Length => out.length = 1,
            Base::Angle => out.angle = 1,
            Base::Time => out.time = 1,
            Base::Percent => out.percent = 1,
        }
        out
    }
    fn combine(self, other: Self, sign: i8) -> Option<Self> {
        Some(Self {
            length: self.length.checked_add(other.length.checked_mul(sign)?)?,
            angle: self.angle.checked_add(other.angle.checked_mul(sign)?)?,
            time: self.time.checked_add(other.time.checked_mul(sign)?)?,
            percent: self.percent.checked_add(other.percent.checked_mul(sign)?)?,
        })
    }
}

/// Unit of a leaf, scaled to its base's canonical unit (`px`, `deg`, `ms`,
/// `%`) at evaluation time.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Unit {
    Number,
    Percent,
    /// A length unit of Values 4 §6 with its factor, or a context length.
    Length(LengthUnit),
    Angle(f64),
    Time(f64),
}

/// The length units of CSS Values 4 §6.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LengthUnit {
    /// An absolute unit (§6.2) in CSS pixels.
    Absolute(f64),
    /// `em` (§6.1.1): the element's font size.
    Em,
    /// `rem` (§6.1.1): the root element's font size.
    Rem,
    /// Viewport-percentage units (§6.1.2): one percent of the width,
    /// height, smaller or larger dimension.
    ViewportWidth,
    ViewportHeight,
    ViewportMin,
    ViewportMax,
    /// Font-relative units the native renderer cannot resolve yet
    /// (`ex`, `ch`, `lh`, …): valid CSS, refused when lowered.
    Unresolved,
}

/// CSS pixels per inch (§6.2).
const PX_PER_INCH: f64 = 96.0;
/// Centimetres per inch.
const CM_PER_INCH: f64 = 2.54;

/// The length unit named `unit` (ASCII case-insensitively), if any.
#[must_use]
pub fn length_unit(unit: &str) -> Option<LengthUnit> {
    use LengthUnit as L;
    let unit = unit.to_ascii_lowercase();
    Some(match unit.as_str() {
        "px" => L::Absolute(1.0),
        "cm" => L::Absolute(PX_PER_INCH / CM_PER_INCH),
        "mm" => L::Absolute(PX_PER_INCH / CM_PER_INCH / 10.0),
        "q" => L::Absolute(PX_PER_INCH / CM_PER_INCH / 40.0),
        "in" => L::Absolute(PX_PER_INCH),
        "pt" => L::Absolute(PX_PER_INCH / 72.0),
        "pc" => L::Absolute(PX_PER_INCH / 6.0),
        "em" => L::Em,
        "rem" => L::Rem,
        "vw" | "svw" | "lvw" | "dvw" => L::ViewportWidth,
        "vh" | "svh" | "lvh" | "dvh" => L::ViewportHeight,
        "vmin" | "svmin" | "lvmin" | "dvmin" => L::ViewportMin,
        "vmax" | "svmax" | "lvmax" | "dvmax" => L::ViewportMax,
        "ex" | "rex" | "cap" | "rcap" | "ch" | "rch" | "ic" | "ric" | "lh" | "rlh" => L::Unresolved,
        _ => return None,
    })
}
/// The angle unit named `unit` in degrees (§7.1).
#[must_use]
pub fn angle_unit(unit: &str) -> Option<f64> {
    Some(match unit.to_ascii_lowercase().as_str() {
        "deg" => 1.0,
        "grad" => 0.9,
        "rad" => 180.0 / std::f64::consts::PI,
        "turn" => 360.0,
        _ => return None,
    })
}
/// The time unit named `unit` in milliseconds (§7.3).
#[must_use]
pub fn time_unit(unit: &str) -> Option<f64> {
    Some(match unit.to_ascii_lowercase().as_str() {
        "ms" => 1.0,
        "s" => 1000.0,
        _ => return None,
    })
}

/// A parsed numeric token: its value and unit.
#[must_use]
pub(crate) fn numeric(token: &V) -> Option<(f64, Unit)> {
    let V::Num { value, unit, .. } = token else {
        return None;
    };
    let unit = match unit {
        super::NumUnit::None => Unit::Number,
        super::NumUnit::Percent => Unit::Percent,
        super::NumUnit::Dimension(name) => {
            if let Some(length) = length_unit(name) {
                Unit::Length(length)
            } else if let Some(degrees) = angle_unit(name) {
                Unit::Angle(degrees)
            } else {
                Unit::Time(time_unit(name)?)
            }
        }
    };
    Some((*value, unit))
}

#[derive(Debug, Clone, PartialEq)]
enum Node {
    Leaf(f64, Unit),
    Sum(Vec<Node>),
    Product(Vec<Node>),
    Negate(Box<Node>),
    Invert(Box<Node>),
    Min(Vec<Node>),
    Max(Vec<Node>),
    Clamp(Box<[Node; 3]>),
}

/// A parsed math function, ready for type checking and evaluation.
#[derive(Debug, Clone, PartialEq)]
pub struct Expression {
    node: Node,
}

/// The context of evaluation (§10.10): what percentages resolve against and
/// the reference lengths. A `None` reference leaves that unit unresolved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Resolution {
    pub em: Option<f64>,
    pub rem: f64,
    pub viewport_width: f64,
    pub viewport_height: f64,
}

/// The value of an evaluated expression in its base's canonical unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Evaluated {
    pub value: f64,
    /// The expression mixes percentages with other leaves of the same
    /// type, so it has no single canonical value before layout.
    pub mixed: bool,
    /// Percentage leaves occur (with `percent_as` resolving them).
    pub percent: bool,
}

struct Parser<'a> {
    tokens: &'a [V],
    at: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&V> {
        self.tokens.get(self.at)
    }
    /// `<calc-sum>` (§10.1); `+` and `-` are delimiters only between two
    /// products, since the tokenizer already reads a signed operand that
    /// lacks whitespace as one number.
    fn sum(&mut self) -> Option<Node> {
        let mut items = vec![self.product()?];
        while let Some(V::Delim(op @ ('+' | '-'))) = self.peek() {
            let negative = *op == '-';
            self.at += 1;
            let item = self.product()?;
            items.push(if negative {
                Node::Negate(Box::new(item))
            } else {
                item
            });
        }
        Some(if items.len() == 1 {
            items.pop()?
        } else {
            Node::Sum(items)
        })
    }
    fn product(&mut self) -> Option<Node> {
        let mut items = vec![self.value()?];
        loop {
            match self.peek() {
                Some(V::Delim('*')) => {
                    self.at += 1;
                    items.push(self.value()?);
                }
                Some(V::Slash) => {
                    self.at += 1;
                    items.push(Node::Invert(Box::new(self.value()?)));
                }
                _ => break,
            }
        }
        Some(if items.len() == 1 {
            items.pop()?
        } else {
            Node::Product(items)
        })
    }
    fn value(&mut self) -> Option<Node> {
        let token = self.peek()?.clone();
        self.at += 1;
        match token {
            V::Num { .. } => {
                let (value, unit) = numeric(&token)?;
                Some(Node::Leaf(value, unit))
            }
            V::Ident(name) => {
                // §10.7.1 numeric constants.
                let value = match name.to_ascii_lowercase().as_str() {
                    "e" => std::f64::consts::E,
                    "pi" => std::f64::consts::PI,
                    "infinity" => f64::INFINITY,
                    "-infinity" => f64::NEG_INFINITY,
                    "nan" => f64::NAN,
                    _ => return None,
                };
                Some(Node::Leaf(value, Unit::Number))
            }
            V::Function(name) if name.is_empty() => {
                let node = self.sum()?;
                self.close()?;
                Some(node)
            }
            V::Function(name) => self.function(&name),
            _ => None,
        }
    }
    fn close(&mut self) -> Option<()> {
        (self.peek() == Some(&V::Close)).then(|| self.at += 1)
    }
    /// A math function after its name token.
    fn function(&mut self, name: &str) -> Option<Node> {
        let name = name.to_ascii_lowercase();
        let mut arguments = vec![self.sum()?];
        while self.peek() == Some(&V::Comma) {
            self.at += 1;
            arguments.push(self.sum()?);
        }
        self.close()?;
        match (name.as_str(), arguments.len()) {
            ("calc", 1) => arguments.pop(),
            ("min", _) => Some(Node::Min(arguments)),
            ("max", _) => Some(Node::Max(arguments)),
            ("clamp", 3) => {
                let [a, b, c]: [Node; 3] = arguments.try_into().ok()?;
                Some(Node::Clamp(Box::new([a, b, c])))
            }
            _ => None,
        }
    }
}

/// Whether `token` opens a math function of the subset.
#[must_use]
pub(crate) fn is_math(token: Option<&V>) -> bool {
    matches!(token, Some(V::Function(name)) if FUNCTIONS.iter().any(|f| name.eq_ignore_ascii_case(f)))
}

/// Parse the math function at `tokens[at]`; returns it and the index past
/// its closing parenthesis.
#[must_use]
pub(crate) fn parse(tokens: &[V], at: usize) -> Option<(Expression, usize)> {
    if !is_math(tokens.get(at)) {
        return None;
    }
    let V::Function(name) = tokens.get(at)? else {
        return None;
    };
    let mut parser = Parser { tokens, at: at + 1 };
    let node = parser.function(name)?;
    Some((Expression { node }, parser.at))
}

impl Expression {
    /// The type of §10.7, with percentages typed as `percent_as` when the
    /// context resolves them against that base (the percent hint).
    #[must_use]
    pub fn resolved_type(&self, percent_as: Option<Base>) -> Option<Type> {
        type_of(&self.node, percent_as)
    }
    /// Evaluate in canonical units (`px`, `deg`, `ms`, `%` or a number).
    /// A percentage leaf counts as its own value in percent; `mixed` reports
    /// a sum of percentages and other leaves, which only layout resolves.
    #[must_use]
    pub fn evaluate(&self, resolution: &Resolution) -> Option<Evaluated> {
        self.evaluate_with(resolution, Flags::default())
    }
    /// Evaluate a `font-size` value: `em` is the parent's font size there
    /// (Fonts 4 §2.5), so it counts as a percentage (`1em` is `100%`).
    #[must_use]
    pub fn evaluate_font_size(&self, resolution: &Resolution) -> Option<Evaluated> {
        self.evaluate_with(
            resolution,
            Flags {
                em_as_percent: true,
                ..Flags::default()
            },
        )
    }
    fn evaluate_with(&self, resolution: &Resolution, mut flags: Flags) -> Option<Evaluated> {
        let value = evaluate(&self.node, resolution, &mut flags)?;
        // §10.12: a top-level NaN becomes zero.
        let value = if value.is_nan() { 0.0 } else { value };
        Some(Evaluated {
            value,
            mixed: flags.percent && flags.other,
            percent: flags.percent,
        })
    }
}

fn type_of(node: &Node, percent_as: Option<Base>) -> Option<Type> {
    match node {
        Node::Leaf(_, unit) => Some(match unit {
            Unit::Number => Type::NUMBER,
            Unit::Percent => Type::of(percent_as.unwrap_or(Base::Percent)),
            Unit::Length(_) => Type::of(Base::Length),
            Unit::Angle(_) => Type::of(Base::Angle),
            Unit::Time(_) => Type::of(Base::Time),
        }),
        Node::Negate(inner) => type_of(inner, percent_as),
        Node::Invert(inner) => Type::NUMBER.combine(type_of(inner, percent_as)?, -1),
        Node::Product(items) => items.iter().try_fold(Type::NUMBER, |acc, item| {
            acc.combine(type_of(item, percent_as)?, 1)
        }),
        Node::Sum(items) | Node::Min(items) | Node::Max(items) => same(items, percent_as),
        Node::Clamp(items) => same(items.as_slice(), percent_as),
    }
}
fn same(items: &[Node], percent_as: Option<Base>) -> Option<Type> {
    let first = type_of(items.first()?, percent_as)?;
    items[1..]
        .iter()
        .all(|item| type_of(item, percent_as) == Some(first))
        .then_some(first)
}

#[derive(Default)]
struct Flags {
    percent: bool,
    other: bool,
    em_as_percent: bool,
}

fn evaluate(node: &Node, resolution: &Resolution, flags: &mut Flags) -> Option<f64> {
    Some(match node {
        Node::Leaf(value, unit) => {
            let scale = match unit {
                Unit::Number => 1.0,
                Unit::Percent => {
                    flags.percent = true;
                    return Some(*value);
                }
                Unit::Angle(factor) | Unit::Time(factor) => *factor,
                Unit::Length(LengthUnit::Em) if flags.em_as_percent => {
                    flags.percent = true;
                    return Some(value * 100.0);
                }
                Unit::Length(length) => {
                    flags.other = true;
                    let vw = resolution.viewport_width / 100.0;
                    let vh = resolution.viewport_height / 100.0;
                    match length {
                        LengthUnit::Absolute(factor) => *factor,
                        LengthUnit::Em => resolution.em?,
                        LengthUnit::Rem => resolution.rem,
                        LengthUnit::ViewportWidth => vw,
                        LengthUnit::ViewportHeight => vh,
                        LengthUnit::ViewportMin => vw.min(vh),
                        LengthUnit::ViewportMax => vw.max(vh),
                        LengthUnit::Unresolved => return None,
                    }
                }
            };
            if matches!(unit, Unit::Angle(_) | Unit::Time(_)) {
                flags.other = true;
            }
            value * scale
        }
        Node::Negate(inner) => -evaluate(inner, resolution, flags)?,
        Node::Invert(inner) => 1.0 / evaluate(inner, resolution, flags)?,
        Node::Sum(items) => items
            .iter()
            .map(|item| evaluate(item, resolution, flags))
            .sum::<Option<f64>>()?,
        // Divide rather than multiply by the inverse, so `1.25 / 0.875` is
        // the correctly rounded quotient.
        Node::Product(items) => items.iter().try_fold(1.0, |acc, item| match item {
            Node::Invert(inner) => Some(acc / evaluate(inner, resolution, flags)?),
            item => Some(acc * evaluate(item, resolution, flags)?),
        })?,
        Node::Min(items) => fold(items, resolution, flags, f64::min)?,
        Node::Max(items) => fold(items, resolution, flags, f64::max)?,
        Node::Clamp(items) => {
            let [low, value, high] = items.as_ref();
            let low = evaluate(low, resolution, flags)?;
            let value = evaluate(value, resolution, flags)?;
            let high = evaluate(high, resolution, flags)?;
            // §10.4: the minimum wins over the maximum; NaN propagates.
            if low.is_nan() || value.is_nan() || high.is_nan() {
                f64::NAN
            } else {
                value.min(high).max(low)
            }
        }
    })
}
/// `min()`/`max()`: NaN in any argument is the result (§10.9).
fn fold(
    items: &[Node],
    resolution: &Resolution,
    flags: &mut Flags,
    pick: fn(f64, f64) -> f64,
) -> Option<f64> {
    let mut out: Option<f64> = None;
    for item in items {
        let value = evaluate(item, resolution, flags)?;
        if value.is_nan() {
            return Some(f64::NAN);
        }
        out = Some(out.map_or(value, |acc| pick(acc, value)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expression(text: &str) -> Expression {
        let tokens = super::super::lex_value(text).unwrap();
        let (expression, end) = parse(&tokens, 0).unwrap();
        assert_eq!(end, tokens.len(), "{text}");
        expression
    }
    const RESOLUTION: Resolution = Resolution {
        em: Some(10.0),
        rem: 16.0,
        viewport_width: 800.0,
        viewport_height: 600.0,
    };

    #[test]
    fn types_follow_the_specification() {
        let length = Some(Type::of(Base::Length));
        assert_eq!(expression("calc(1px + 2rem)").resolved_type(None), length);
        assert_eq!(expression("calc(2 * 3px)").resolved_type(None), length);
        assert_eq!(
            expression("calc(1px / 1px)").resolved_type(None),
            Some(Type::NUMBER)
        );
        assert_eq!(expression("calc(1px + 1)").resolved_type(None), None);
        assert_eq!(expression("calc(100% - 1px)").resolved_type(None), None);
        assert_eq!(
            expression("calc(100% - 1px)").resolved_type(Some(Base::Length)),
            length
        );
        assert_eq!(
            expression("clamp(1px, 2vw, 3rem)").resolved_type(None),
            length
        );
        assert_eq!(
            expression("calc(1s + 100ms)").resolved_type(None),
            Some(Type::of(Base::Time))
        );
        // `+` needs whitespace: `1px +2px` is two operands without an operator.
        let tokens = super::super::lex_value("calc(1px +2px)").unwrap();
        assert!(parse(&tokens, 0).is_none());
        let tokens = super::super::lex_value("round(1px, 2px)").unwrap();
        assert!(parse(&tokens, 0).is_none());
    }

    #[test]
    fn evaluation_resolves_units_constants_and_edge_values() {
        let value = |text: &str| expression(text).evaluate(&RESOLUTION).unwrap().value;
        assert_eq!(value("calc(0.25rem * 4)"), 16.0);
        assert_eq!(value("calc(1in - 90px)"), 6.0);
        assert_eq!(value("calc(10vw + 10vh)"), 140.0);
        assert_eq!(value("max(1em, 12px)"), 12.0);
        assert_eq!(value("clamp(10px, 50px, 20px)"), 20.0);
        assert_eq!(value("clamp(30px, 50px, 20px)"), 30.0);
        assert_eq!(value("calc(1.25 / 0.875)"), 1.25 / 0.875);
        assert_eq!(value("calc((1px + 2px) * -1)"), -3.0);
        assert_eq!(value("calc(infinity * 1px)"), f64::INFINITY);
        assert_eq!(value("calc(0 / 0)"), 0.0);
        assert_eq!(value("calc(1s + 1ms)"), 1001.0);
        assert!(
            expression("calc(100% - 1px)")
                .evaluate(&RESOLUTION)
                .unwrap()
                .mixed
        );
        assert!(
            !expression("calc(50% * 2)")
                .evaluate(&RESOLUTION)
                .unwrap()
                .mixed
        );
        assert!(expression("calc(1lh)").evaluate(&RESOLUTION).is_none());
    }
}
