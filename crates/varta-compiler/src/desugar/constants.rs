//! Constant enum member expressions (TypeScript's constant enum expressions):
//! literals, no-substitution templates, references to earlier members of
//! the same enum or to members of other enums, parentheses, the unary
//! `+ - ~` and the binary `+ - * / % ** << >> >>> & | ^` operators with
//! ECMAScript number semantics, string concatenation and the `length` of a
//! string literal.
use super::{Symbol, Value};
use swc_ecma_ast as ast;

/// Members evaluated so far and the enums visible to the initializer.
pub(super) struct Context<'a> {
    pub own: &'a str,
    pub members: &'a [(String, Value)],
    pub lookup: &'a dyn Fn(&str) -> Option<Symbol>,
}

pub(super) fn evaluate(expr: &ast::Expr, context: &Context<'_>) -> Option<Value> {
    match expr {
        ast::Expr::Lit(ast::Lit::Num(number)) => Some(Value::Number(number.value)),
        ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().map(|s| Value::String(s.into())),
        ast::Expr::Tpl(template) if template.exprs.is_empty() => template
            .quasis
            .first()
            .and_then(|quasi| quasi.cooked.as_ref())
            .and_then(|cooked| cooked.as_str())
            .map(|s| Value::String(s.into())),
        ast::Expr::Paren(paren) => evaluate(&paren.expr, context),
        ast::Expr::Ident(id) => {
            let name = id.sym.as_ref();
            match name {
                "Infinity" if (context.lookup)(name).is_none() => {
                    return Some(Value::Number(f64::INFINITY));
                }
                "NaN" if (context.lookup)(name).is_none() => return Some(Value::Number(f64::NAN)),
                _ => {}
            }
            own(context, name)
        }
        ast::Expr::Member(member) => {
            let key = key(&member.prop)?;
            if let ast::Expr::Lit(ast::Lit::Str(text)) = &*member.obj
                && key == "length"
            {
                // UTF-16 length, as `String.prototype.length`.
                let text = text.value.as_str()?;
                return Some(Value::Number(text.encode_utf16().count() as f64));
            }
            let ast::Expr::Ident(base) = &*member.obj else {
                return None;
            };
            if base.sym == context.own {
                return own(context, &key);
            }
            let symbol = (context.lookup)(base.sym.as_ref())?;
            let enumeration = symbol.enumeration?;
            enumeration
                .members
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| value.clone())
        }
        ast::Expr::Unary(unary) => {
            let Value::Number(value) = evaluate(&unary.arg, context)? else {
                return None;
            };
            match unary.op {
                ast::UnaryOp::Minus => Some(Value::Number(-value)),
                ast::UnaryOp::Plus => Some(Value::Number(value)),
                ast::UnaryOp::Tilde => Some(Value::Number(f64::from(!to_int32(value)))),
                _ => None,
            }
        }
        ast::Expr::Bin(binary) => {
            let left = evaluate(&binary.left, context)?;
            let right = evaluate(&binary.right, context)?;
            binary_value(binary.op, left, right)
        }
        _ => None,
    }
}

fn own(context: &Context<'_>, name: &str) -> Option<Value> {
    context
        .members
        .iter()
        .find(|(member, _)| member == name)
        .map(|(_, value)| value.clone())
}

fn key(prop: &ast::MemberProp) -> Option<String> {
    match prop {
        ast::MemberProp::Ident(id) => Some(id.sym.to_string()),
        ast::MemberProp::Computed(computed) => match &*computed.expr {
            ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().map(str::to_owned),
            _ => None,
        },
        ast::MemberProp::PrivateName(_) => None,
    }
}

fn binary_value(op: ast::BinaryOp, left: Value, right: Value) -> Option<Value> {
    use ast::BinaryOp as B;
    if op == B::Add
        && let (Value::String(left), Value::String(right)) = (&left, &right)
    {
        return Some(Value::String(format!("{left}{right}")));
    }
    let (Value::Number(a), Value::Number(b)) = (left, right) else {
        return None;
    };
    let value = match op {
        B::Add => a + b,
        B::Sub => a - b,
        B::Mul => a * b,
        B::Div => a / b,
        B::Mod => a % b,
        B::Exp => power(a, b),
        B::LShift => f64::from(to_int32(a).wrapping_shl(to_uint32(b) & 31)),
        B::RShift => f64::from(to_int32(a).wrapping_shr(to_uint32(b) & 31)),
        B::ZeroFillRShift => f64::from(to_uint32(a).wrapping_shr(to_uint32(b) & 31)),
        B::BitAnd => f64::from(to_int32(a) & to_int32(b)),
        B::BitOr => f64::from(to_int32(a) | to_int32(b)),
        B::BitXor => f64::from(to_int32(a) ^ to_int32(b)),
        _ => return None,
    };
    Some(Value::Number(value))
}

/// `Number::exponentiate` (ECMAScript 6.1.6.1.3): a NaN exponent gives NaN
/// and `(±1) ** ±Infinity` is NaN, unlike `powf`.
fn power(base: f64, exponent: f64) -> f64 {
    if exponent.is_nan() || base.abs() == 1.0 && exponent.is_infinite() {
        return f64::NAN;
    }
    base.powf(exponent)
}

/// ECMAScript `ToInt32`.
pub(super) fn to_int32(value: f64) -> i32 {
    to_uint32(value) as i32
}

/// ECMAScript `ToUint32`.
fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    let modulus = 4_294_967_296.0;
    let integer = value.trunc() % modulus;
    let positive = if integer < 0.0 {
        integer + modulus
    } else {
        integer
    };
    positive as u32
}
