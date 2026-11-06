//! `enum` declarations: evaluation of the member values and the generated
//! type alias and reverse-mapping function.
use super::constants::{self, Context};
use super::{Enum, Symbol, Value, type_error, unsupported};
use crate::CompilerError;
use crate::source::Source;
use std::fmt::Write as _;
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_visit::{VisitMut, VisitMutWith};

/// Evaluate the members of `decl`, bound under `name`; `reverse` names the
/// reverse-mapping function of a non-const enum with numeric members.
pub(super) fn evaluate(
    decl: &ast::TsEnumDecl,
    name: &str,
    (reverse, object): (String, String),
    source: &Source,
    lookup: &dyn Fn(&str) -> Option<Symbol>,
) -> Result<Enum, CompilerError> {
    if decl.declare {
        return Err(source.locate(unsupported("ambient enum declaration"), decl.span));
    }
    if decl.members.is_empty() {
        return Err(source.locate(unsupported("enum without members"), decl.span));
    }
    let own = decl.id.sym.to_string();
    let mut members: Vec<(String, Value)> = Vec::new();
    for member in &decl.members {
        let member_name = match &member.id {
            ast::TsEnumMemberId::Ident(id) => id.sym.to_string(),
            ast::TsEnumMemberId::Str(text) => text
                .value
                .as_str()
                .ok_or_else(|| source.locate(unsupported("non-UTF-8 enum member name"), text.span))?
                .to_owned(),
        };
        if members.iter().any(|(existing, _)| *existing == member_name) {
            return Err(source.locate(
                CompilerError::Invalid(format!("duplicate enum member: {member_name}")),
                member.span,
            ));
        }
        let value = match &member.init {
            None => match members.last() {
                None => Value::Number(0.0),
                Some((_, Value::Number(previous))) => Value::Number(previous + 1.0),
                Some((_, Value::String(_))) => {
                    return Err(source.locate(
                        type_error(format!(
                            "enum member {member_name} must have an initializer (TS1061)"
                        )),
                        member.span,
                    ));
                }
            },
            Some(init) => {
                let context = Context {
                    own: &own,
                    members: &members,
                    lookup,
                };
                match constants::evaluate(init, &context) {
                    Some(value) => value,
                    None if decl.is_const => {
                        return Err(source.locate(
                            type_error(
                                "const enum member initializers must be constant expressions (TS2474)",
                            ),
                            init.span(),
                        ));
                    }
                    None => {
                        return Err(source.locate(
                            unsupported(
                                "computed enum member (an initializer that is not a constant expression)",
                            ),
                            init.span(),
                        ));
                    }
                }
            }
        };
        if let Value::Number(number) = value
            && !number.is_finite()
        {
            return Err(source.locate(
                unsupported("enum member with a non-finite value"),
                member.span,
            ));
        }
        members.push((member_name, value));
    }
    let numeric = members.iter().any(|(_, v)| matches!(v, Value::Number(_)));
    Ok(Enum {
        name: name.to_owned(),
        is_const: decl.is_const,
        object: (!decl.is_const && members.iter().all(|(_, v)| object_key(v).is_some()))
            .then_some(object),
        reverse: (numeric && !decl.is_const).then_some(reverse),
        members,
    })
}

/// The property key a member value has in the enum object's reverse
/// mapping (`E[E.A = 0] = "A"` stores under `ToString(0)`): exact for
/// integers within ±2^53; other numbers have no object here.
fn object_key(value: &Value) -> Option<String> {
    match value {
        Value::String(_) => Some(String::new()),
        Value::Number(n) if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 => {
            Some(format!("{}", *n as i64))
        }
        Value::Number(_) => None,
    }
}

/// Whether a property key is an array index, which JavaScript enumerates
/// first, in ascending order.
fn array_index(key: &str) -> Option<u32> {
    let index: u32 = key.parse().ok()?;
    (index != u32::MAX && index.to_string() == key).then_some(index)
}

/// `const E$object = { … }`: the enum object, with JavaScript's own-key
/// order (array-index keys ascending, then the others in creation order:
/// each member's name, then its reverse-mapping key).
fn object(enumeration: &Enum, name: &str, span: Span) -> ast::Decl {
    let mut entries: Vec<(String, Value)> = Vec::new();
    let mut put = |key: String, value: Value| match entries.iter_mut().find(|(k, _)| *k == key) {
        Some(entry) => entry.1 = value,
        None => entries.push((key, value)),
    };
    for (member, value) in &enumeration.members {
        put(member.clone(), value.clone());
        if let Value::Number(_) = value
            && let Some(key) = object_key(value)
        {
            put(key, Value::String(member.clone()));
        }
    }
    let (mut indices, others): (Vec<_>, Vec<_>) = entries
        .into_iter()
        .partition(|(k, _)| array_index(k).is_some());
    indices.sort_by_key(|(k, _)| array_index(k));
    let props = indices
        .into_iter()
        .chain(others)
        .map(|(key, value)| {
            ast::PropOrSpread::Prop(Box::new(ast::Prop::KeyValue(ast::KeyValueProp {
                key: ast::PropName::Str(string(&key, span)),
                value: Box::new(literal(&value, span)),
            })))
        })
        .collect();
    ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(ast::BindingIdent {
                id: ast::Ident::new_no_ctxt(name.into(), span),
                type_ann: None,
            }),
            init: Some(Box::new(ast::Expr::Object(ast::ObjectLit { span, props }))),
            definite: false,
        }],
    }))
}

/// The literal expression of a member value.
pub(super) fn literal(value: &Value, span: Span) -> ast::Expr {
    match value {
        Value::String(text) => ast::Expr::Lit(ast::Lit::Str(string(text, span))),
        Value::Number(number) if number.is_sign_negative() => ast::Expr::Unary(ast::UnaryExpr {
            span,
            op: ast::UnaryOp::Minus,
            arg: Box::new(ast::Expr::Lit(ast::Lit::Num(ast::Number {
                span,
                value: -number,
                raw: None,
            }))),
        }),
        Value::Number(number) => ast::Expr::Lit(ast::Lit::Num(ast::Number {
            span,
            value: *number,
            raw: None,
        })),
    }
}

pub(super) fn string(text: &str, span: Span) -> ast::Str {
    ast::Str {
        span,
        value: text.into(),
        raw: None,
    }
}

/// The literal type of a member value.
pub(super) fn literal_type(value: &Value, span: Span) -> ast::TsType {
    let lit = match value {
        Value::String(text) => ast::TsLit::Str(string(text, span)),
        Value::Number(number) => ast::TsLit::Number(ast::Number {
            span,
            value: *number,
            raw: None,
        }),
    };
    ast::TsType::TsLitType(ast::TsLitType { span, lit })
}

/// The enum type: the union of its member literal types.
fn union(enumeration: &Enum, span: Span) -> ast::TsType {
    let mut types: Vec<Box<ast::TsType>> = Vec::new();
    for (_, value) in &enumeration.members {
        let ty = literal_type(value, span);
        if !types.iter().any(|t| **t == ty) {
            types.push(Box::new(ty));
        }
    }
    if types.len() == 1 {
        return *types.remove(0);
    }
    ast::TsType::TsUnionOrIntersectionType(ast::TsUnionOrIntersectionType::TsUnionType(
        ast::TsUnionType { span, types },
    ))
}

/// The declarations an enum lowers to: its type alias and, for a non-const
/// enum with numeric members, the reverse-mapping function.
pub(super) fn items(
    enumeration: &Enum,
    exported: bool,
    span: Span,
    source: &Source,
) -> Result<Vec<ast::ModuleItem>, CompilerError> {
    let alias = ast::Decl::TsTypeAlias(Box::new(ast::TsTypeAliasDecl {
        span,
        declare: false,
        id: ast::Ident::new_no_ctxt(enumeration.name.as_str().into(), span),
        type_params: None,
        type_ann: Box::new(union(enumeration, span)),
    }));
    let mut items = vec![super::namespaces::item(alias, exported, span)];
    if let Some(name) = &enumeration.object {
        items.push(super::namespaces::item(
            object(enumeration, name, span),
            exported,
            span,
        ));
    }
    if let Some(reverse) = &enumeration.reverse {
        items.push(super::namespaces::item(
            reverse_function(enumeration, reverse, span, source)?,
            exported,
            span,
        ));
    }
    Ok(items)
}

/// `function reverse(value: number): string | undefined`: the name of the
/// last member with that numeric value, as the enum object's reverse
/// mapping holds it.
fn reverse_function(
    enumeration: &Enum,
    name: &str,
    span: Span,
    source: &Source,
) -> Result<ast::Decl, CompilerError> {
    let mut cases: Vec<(f64, &str)> = Vec::new();
    for (member, value) in &enumeration.members {
        if let Value::Number(number) = value {
            match cases.iter_mut().find(|(n, _)| n == number) {
                Some(case) => case.1 = member,
                None => cases.push((*number, member)),
            }
        }
    }
    let mut text = format!("function {name}(value: number): string | undefined {{\n");
    text.push_str("  switch (value) {\n");
    for (number, member) in cases {
        // `{:?}` keeps the exact binary64 value (`1e21`, `0.1`, `-3.0`).
        let _ = writeln!(text, "    case {number:?}: return {};", quoted(member));
    }
    text.push_str("  }\n  return undefined;\n}\n");
    let path = std::path::PathBuf::from(source.file.name.to_string());
    let mut module = crate::parse::parse_module(&Source::new(&path, &text), false)
        .map_err(|e| CompilerError::Internal(format!("generated enum mapping: {e}")))?;
    module.visit_mut_with(&mut Respan(span));
    match module.body.pop() {
        Some(ast::ModuleItem::Stmt(ast::Stmt::Decl(decl))) => Ok(decl),
        _ => Err(CompilerError::Internal(
            "generated enum reverse mapping".into(),
        )),
    }
}

/// A JavaScript string literal of `text`.
fn quoted(text: &str) -> String {
    let mut quoted = String::from("\"");
    for c in text.chars() {
        match c {
            '"' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            c if (c as u32) < 0x20 || c == '\u{2028}' || c == '\u{2029}' => {
                let _ = write!(quoted, "\\u{:04x}", c as u32);
            }
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

/// Give every node of generated code the span of its declaration.
pub(super) struct Respan(pub Span);
impl VisitMut for Respan {
    fn visit_mut_span(&mut self, span: &mut Span) {
        *span = self.0;
    }
}
