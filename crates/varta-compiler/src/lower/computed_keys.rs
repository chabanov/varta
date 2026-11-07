//! V38 constant computed keys of object literals (`expression_forms`).
//!
//! `{ [k]: v }` names a closed record field when TypeScript knows the key
//! statically: a string or number literal, a template without
//! substitutions, or a binding whose type is a single string or number
//! literal (`const k = "a"`, a module constant). The key is the ToString of
//! the constant, as ECMAScript ToPropertyKey gives it. Keys known only at
//! run time need an index-signature (dictionary) context, and symbol keys
//! stay refused.
use super::*;
use literals::LiteralValue;

impl Lower<'_> {
    /// `object` with every constant computed key replaced by its name, or
    /// `None` when it has no computed key.
    pub(super) fn constant_keys(&self, object: &ast::ObjectLit) -> Result<Option<ast::ObjectLit>> {
        let computed = |prop: &ast::PropOrSpread| match prop {
            ast::PropOrSpread::Prop(prop) => {
                matches!(property_name(prop), Some(ast::PropName::Computed(_)))
            }
            ast::PropOrSpread::Spread(_) => false,
        };
        if !object.props.iter().any(computed) {
            return Ok(None);
        }
        let mut rewritten = object.clone();
        for prop in &mut rewritten.props {
            let ast::PropOrSpread::Prop(prop) = prop else {
                continue;
            };
            let Some(name) = property_name_mut(prop) else {
                continue;
            };
            let ast::PropName::Computed(key) = name else {
                continue;
            };
            let span = key.span;
            let text = self.constant_key(&key.expr).ok_or_else(|| {
                self.source.locate(
                    unsupported(
                        "computed object key that is not a string or number constant (a key known only at run time needs an index signature)",
                    ),
                    span,
                )
            })?;
            *name = ast::PropName::Str(ast::Str {
                span,
                value: text.into(),
                raw: None,
            });
        }
        Ok(Some(rewritten))
    }

    /// The property key of a constant key expression.
    pub(super) fn constant_key(&self, expr: &ast::Expr) -> Option<String> {
        match stmt::unparen(expr) {
            ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().map(str::to_owned),
            ast::Expr::Lit(ast::Lit::Num(number)) => Some(number_key(number.value)),
            ast::Expr::Tpl(template) if template.exprs.is_empty() => template
                .quasis
                .first()
                .and_then(|quasi| quasi.cooked.as_ref())
                .and_then(|cooked| cooked.as_str().map(str::to_owned)),
            ast::Expr::Ident(name) => {
                if let Ok(binding) = self.binding(name.sym.as_ref())
                    && let Some(key) = self.types.constant_keys.get(&binding.identity)
                {
                    return Some(key.clone());
                }
                let Some(Type::Literal(_, id)) = self.peek_type(expr, 0) else {
                    return None;
                };
                let values = self.types.literals.values(id);
                if values.len() != 1 {
                    return None;
                }
                match values.iter().next()? {
                    LiteralValue::String(bytes) => self.key_text(bytes),
                    LiteralValue::Number(bits) => Some(number_key(f64::from_bits(*bits))),
                    LiteralValue::Boolean(_) => None,
                }
            }
            _ => None,
        }
    }
    /// Remember the key of a `const` binding initialized by a literal.
    pub(super) fn note_constant_key(&mut self, identity: (usize, u32), value: &Expr) {
        if let Some(key) = self.constant_key_of(value) {
            self.types.constant_keys.insert(identity, key);
        }
    }
    /// The property key of a string or number constant expression.
    pub(super) fn constant_key_of(&self, value: &Expr) -> Option<String> {
        match (&value.kind, value.ty.erased()) {
            (E::String(bytes), Type::String) => self.key_text(bytes),
            (E::Float64(number), Type::Float64 | Type::Number) => Some(number_key(*number)),
            (E::Integer(number), Type::Number) => Some(number.to_string()),
            _ => None,
        }
    }
    fn key_text(&self, bytes: &[u8]) -> Option<String> {
        if self.types.profile.utf16() {
            let units: Vec<u16> = bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_le_bytes(*pair))
                .collect();
            String::from_utf16(&units).ok()
        } else {
            String::from_utf8(bytes.to_vec()).ok()
        }
    }
}

/// ECMAScript Number::toString for a key: integers print without a
/// fraction; other values use Rust's shortest round-trip form, which
/// matches JavaScript for the decimal range keys use.
fn number_key(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e21 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

fn property_name(prop: &ast::Prop) -> Option<&ast::PropName> {
    match prop {
        ast::Prop::KeyValue(pair) => Some(&pair.key),
        ast::Prop::Method(method) => Some(&method.key),
        ast::Prop::Getter(getter) => Some(&getter.key),
        ast::Prop::Setter(setter) => Some(&setter.key),
        _ => None,
    }
}
fn property_name_mut(prop: &mut ast::Prop) -> Option<&mut ast::PropName> {
    match prop {
        ast::Prop::KeyValue(pair) => Some(&mut pair.key),
        ast::Prop::Method(method) => Some(&mut method.key),
        ast::Prop::Getter(getter) => Some(&mut getter.key),
        ast::Prop::Setter(setter) => Some(&mut setter.key),
        _ => None,
    }
}
