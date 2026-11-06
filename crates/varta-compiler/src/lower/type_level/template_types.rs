//! Template literal types (`` `on${Name}` ``) and the intrinsic string
//! manipulation types `Uppercase`, `Lowercase`, `Capitalize` and
//! `Uncapitalize` (TypeScript Handbook, Template Literal Types).
//!
//! A template whose placeholders are finite literal sets evaluates to the
//! string literal set of the cross product. A placeholder of `string`,
//! `number` or `bigint` would make a pattern literal type, which has no
//! finite member set and no static representation here: it is admitted only
//! as an `extends` pattern of a conditional type, where it is matched
//! against string literals.
use super::*;
use crate::lower::literals::LiteralValue;
use ir::LiteralBase;
use std::collections::BTreeSet;

/// The text of one template piece in UTF-16 code units.
fn quasi(element: &ast::TplElement) -> Result<Vec<u16>> {
    let cooked = element
        .cooked
        .as_ref()
        .ok_or_else(|| invalid("template literal type with an invalid escape"))?;
    Ok(cooked.as_wtf8().to_ill_formed_utf16().collect())
}

/// The finite texts of a placeholder type, in UTF-16 code units.
fn texts(ty: Type, types: &mut ir::ValueTypes) -> Result<Vec<Vec<u16>>> {
    let mut texts = Vec::new();
    for member in split(ty, types) {
        let text = match member {
            Type::Literal(_, id) => {
                let value = types
                    .literals
                    .values(id)
                    .first()
                    .cloned()
                    .ok_or_else(|| invalid("empty literal type"))?;
                match &value {
                    LiteralValue::String(_) => literals::string_units(&value, types.profile)
                        .ok_or_else(|| invalid("malformed string literal type"))?,
                    LiteralValue::Number(bits) => {
                        number_text(f64::from_bits(*bits)).encode_utf16().collect()
                    }
                    LiteralValue::Boolean(flag) => flag.to_string().encode_utf16().collect(),
                }
            }
            Type::Null => "null".encode_utf16().collect(),
            Type::Undefined => "undefined".encode_utf16().collect(),
            Type::Never => continue,
            other => {
                return Err(unsupported(&format!(
                    "template literal type with a {} placeholder (a pattern literal type has no finite member set; it is admitted only as a conditional type's extends pattern)",
                    subtyping::describe(other, types)
                )));
            }
        };
        texts.push(text);
    }
    Ok(texts)
}

/// `` `a${X}b${Y}` ``: the string literal set of every combination.
pub(super) fn evaluate(template: &ast::TsTplLitType, types: &mut ir::ValueTypes) -> Result<Type> {
    let mut results: Vec<Vec<u16>> = vec![quasi(&template.quasis[0])?];
    for (placeholder, next) in template.types.iter().zip(&template.quasis[1..]) {
        let ty = keyword_type(placeholder, types)?;
        let choices = texts(ty, types)?;
        let tail = quasi(next)?;
        if results.len().saturating_mul(choices.len()) > literals::MAX_LITERAL_MEMBERS {
            return Err(invalid(format!(
                "template literal type has more than {} members",
                literals::MAX_LITERAL_MEMBERS
            )));
        }
        let mut combined = Vec::new();
        for prefix in &results {
            for choice in &choices {
                let mut text = prefix.clone();
                text.extend_from_slice(choice);
                text.extend_from_slice(&tail);
                combined.push(text);
            }
        }
        results = combined;
    }
    if results.is_empty() {
        return Ok(Type::Never);
    }
    let values: BTreeSet<LiteralValue> = results
        .iter()
        .map(|units| literals::string_value(units, types.profile))
        .collect();
    literals::of_values(LiteralBase::String, values, types)
}

/// One placeholder of a template pattern.
enum Hole<'a> {
    /// `infer X` (with an optional constraint).
    Infer(&'a ast::TsInferType),
    /// A type the matched text must belong to (`string`, `number`, literals).
    Type(Type),
}

/// Match a string literal against a template pattern, binding its `infer`
/// placeholders. A placeholder followed by text takes the text up to the
/// first occurrence of that text; one followed directly by another takes a
/// single code unit; the last takes the rest (TypeScript's inference).
pub(super) fn matches(
    template: &ast::TsTplLitType,
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let Type::Literal(LiteralBase::String, id) = ty else {
        return Ok(false);
    };
    let values: Vec<_> = types.literals.values(id).iter().cloned().collect();
    let [value] = &values[..] else {
        // A set matches when every member does; bind the union of each.
        let mut all = true;
        for value in values {
            let single = literals::single(LiteralBase::String, value, types)?;
            all &= matches(template, single, bindings, types)?;
        }
        return Ok(all);
    };
    let text = literals::string_units(value, types.profile)
        .ok_or_else(|| invalid("malformed string literal type"))?;
    let mut holes = Vec::new();
    for placeholder in &template.types {
        holes.push(match &**placeholder {
            ast::TsType::TsInferType(infer) => Hole::Infer(infer),
            other => Hole::Type(with_bindings(other, bindings, types)?),
        });
    }
    let quasis = template
        .quasis
        .iter()
        .map(quasi)
        .collect::<Result<Vec<_>>>()?;
    let (first, last) = (&quasis[0], &quasis[quasis.len() - 1]);
    if !text.starts_with(first) {
        return Ok(false);
    }
    if holes.is_empty() {
        return Ok(text == *first);
    }
    if text.len() < first.len() + last.len() || !text.ends_with(last) {
        return Ok(false);
    }
    let end = text.len() - last.len();
    let mut position = first.len();
    for (index, hole) in holes.iter().enumerate() {
        let following = &quasis[index + 1];
        let piece_end = if index + 1 == holes.len() {
            end
        } else if following.is_empty() {
            if position >= end {
                return Ok(false);
            }
            position + 1
        } else {
            match find(&text[position..end], following) {
                Some(offset) => position + offset,
                None => return Ok(false),
            }
        };
        let piece = &text[position..piece_end];
        if !hole_matches(hole, piece, bindings, types)? {
            return Ok(false);
        }
        position = piece_end
            + if index + 1 == holes.len() {
                0
            } else {
                following.len()
            };
    }
    Ok(true)
}

fn find(haystack: &[u16], needle: &[u16]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn hole_matches(
    hole: &Hole<'_>,
    piece: &[u16],
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let string = literals::single(
        LiteralBase::String,
        literals::string_value(piece, types.profile),
        types,
    )?;
    match hole {
        Hole::Type(ty) => Ok(piece_belongs(piece, *ty, types)),
        Hole::Infer(infer) => {
            let name = infer.type_param.name.sym.to_string();
            let mut candidate = string;
            if let Some(constraint) = &infer.type_param.constraint {
                let bound = with_bindings(constraint, bindings, types)?;
                // `infer N extends number` converts a round-tripping numeric
                // text to its number literal (TypeScript 4.8).
                if !extends(candidate, bound, types) {
                    let text = String::from_utf16_lossy(piece);
                    let numeric = numeric_text(&text)
                        .and_then(literals::number_value)
                        .map(|value| literals::single(LiteralBase::Number, value, types))
                        .transpose()?;
                    match numeric {
                        Some(number) if extends(number, bound, types) => candidate = number,
                        _ => return Ok(false),
                    }
                }
            }
            let candidate = match bindings.get(&name) {
                Some(previous) if *previous != candidate => {
                    union_of(vec![*previous, candidate], types)?
                }
                _ => candidate,
            };
            bindings.insert(name, candidate);
            Ok(true)
        }
    }
}

/// Whether the text `piece` is a value of the placeholder type `ty`.
fn piece_belongs(piece: &[u16], ty: Type, types: &mut ir::ValueTypes) -> bool {
    let text = String::from_utf16_lossy(piece);
    split(ty, types).into_iter().any(|member| match member {
        Type::String => true,
        Type::Number | Type::Float64 => numeric_text(&text).is_some(),
        Type::Literal(_, id) => types.literals.values(id).iter().any(|value| match value {
            LiteralValue::String(_) => {
                literals::string_units(value, types.profile).is_some_and(|units| units == piece)
            }
            LiteralValue::Number(bits) => number_text(f64::from_bits(*bits)) == text,
            LiteralValue::Boolean(flag) => flag.to_string() == text,
        }),
        Type::Null => text == "null",
        Type::Undefined => text == "undefined",
        _ => false,
    })
}

/// The number of a text that converts back to the same text
/// (TypeScript's `isValidNumberString` with round-tripping).
fn numeric_text(text: &str) -> Option<f64> {
    let value: f64 = text.trim().parse().ok()?;
    (value.is_finite() && number_text(value) == text).then_some(value)
}

/// ECMAScript `Number::toString(x)` in radix 10.
pub(super) fn number_text(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == 0.0 {
        return "0".into();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    // Shortest round-tripping digits and exponent: d.ddd e k.
    let scientific = format!("{:e}", value.abs());
    let (mantissa, exponent) = scientific
        .split_once('e')
        .unwrap_or((scientific.as_str(), "0"));
    let digits: String = mantissa.chars().filter(char::is_ascii_digit).collect();
    let exponent: i32 = exponent.parse().unwrap_or(0);
    let k = digits.len() as i32;
    let n = exponent + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let sign = if n - 1 < 0 { '-' } else { '+' };
        let (head, tail) = digits.split_at(1);
        let fraction = if tail.is_empty() {
            String::new()
        } else {
            format!(".{tail}")
        };
        format!("{head}{fraction}e{sign}{}", (n - 1).abs())
    };
    format!("{sign}{body}")
}

/// The intrinsic string types over a string literal set.
#[derive(Clone, Copy)]
pub(super) enum Intrinsic {
    Uppercase,
    Lowercase,
    Capitalize,
    Uncapitalize,
}
impl Intrinsic {
    pub(super) fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "Uppercase" => Self::Uppercase,
            "Lowercase" => Self::Lowercase,
            "Capitalize" => Self::Capitalize,
            "Uncapitalize" => Self::Uncapitalize,
            _ => return None,
        })
    }
    fn name(self) -> &'static str {
        match self {
            Self::Uppercase => "Uppercase",
            Self::Lowercase => "Lowercase",
            Self::Capitalize => "Capitalize",
            Self::Uncapitalize => "Uncapitalize",
        }
    }
    /// JavaScript's `toUpperCase`/`toLowerCase` over UTF-16 code units
    /// (whole string, or only the first code unit for (Un)capitalize).
    fn apply(self, units: &[u16]) -> Vec<u16> {
        let convert = |units: &[u16], upper: bool| -> Vec<u16> {
            char::decode_utf16(units.iter().copied())
                .flat_map(|decoded| match decoded {
                    Ok(c) => {
                        let mapped: String = if upper {
                            c.to_uppercase().collect()
                        } else {
                            lower_case(c)
                        };
                        mapped.encode_utf16().collect::<Vec<_>>()
                    }
                    Err(lone) => vec![lone.unpaired_surrogate()],
                })
                .collect()
        };
        match self {
            Self::Uppercase => convert(units, true),
            Self::Lowercase => convert(units, false),
            Self::Capitalize | Self::Uncapitalize => {
                let Some((first, rest)) = units.split_first() else {
                    return Vec::new();
                };
                let mut out = convert(
                    std::slice::from_ref(first),
                    matches!(self, Self::Capitalize),
                );
                out.extend_from_slice(rest);
                out
            }
        }
    }
    pub(super) fn evaluate(self, ty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
        let mut values = BTreeSet::new();
        for member in split(ty, types) {
            let Type::Literal(LiteralBase::String, id) = member else {
                if member == Type::Never {
                    continue;
                }
                return Err(unsupported(&format!(
                    "{}<{}> (an intrinsic string type over a non-literal type has no finite member set)",
                    self.name(),
                    subtyping::describe(member, types)
                )));
            };
            for value in types.literals.values(id).clone() {
                let units = literals::string_units(&value, types.profile)
                    .ok_or_else(|| invalid("malformed string literal type"))?;
                values.insert(literals::string_value(&self.apply(&units), types.profile));
            }
        }
        if values.is_empty() {
            return Ok(Type::Never);
        }
        literals::of_values(LiteralBase::String, values, types)
    }
}

/// `toLowerCase` of one character: Unicode's default mapping, with the final
/// sigma rule of JavaScript left to its context-free form.
fn lower_case(c: char) -> String {
    c.to_lowercase().collect()
}
