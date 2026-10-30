//! Function assignability to `void` results (v37 `language_core`).
//!
//! As in TypeScript, a function whose result is any value is assignable to a
//! function type with the same parameters and a `void` result; callers through
//! the `void` type ignore the value. Function values carry their exact
//! signature (the closed call dispatch checks it), so the conversion wraps the
//! value once in a synthesized closure that calls it and discards the result:
//! the value is evaluated where it is converted, into a hidden binding the
//! wrapper captures. Only fixed-arity functions without a receiver convert.
//!
//! Mismatch diagnostics between two function types name both signatures.
use super::*;

/// Name prefix of the hidden binding of a converted function value and of the
/// wrapper's parameters; `#` cannot start a source identifier.
const CONVERTED: &str = "#void";

/// `ty` as TypeScript-like source text: function types show their signature.
pub(super) fn describe(ty: Type, types: &ir::ValueTypes) -> String {
    describe_depth(ty, types, 0)
}
fn describe_depth(ty: Type, types: &ir::ValueTypes, depth: usize) -> String {
    /// Nested function types beyond this depth are abbreviated.
    const MAX_DEPTH: usize = 4;
    if depth >= MAX_DEPTH {
        return ty.to_string();
    }
    let id = match ty {
        Type::Function(id) => id,
        Type::Array(id) => {
            return format!(
                "{}[]",
                describe_depth(types.arrays[id as usize], types, depth + 1)
            );
        }
        Type::Union(id) | Type::ReferenceUnion(id) => {
            return types.unions[id as usize]
                .iter()
                .map(|member| describe_depth(*member, types, depth + 1))
                .collect::<Vec<_>>()
                .join(" | ");
        }
        // A record shape by its field names (v37 union diagnostics).
        Type::Record(id) => {
            let names: Vec<&str> = types.records[id as usize]
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect();
            return format!("{{ {} }}", names.join(", "));
        }
        _ => return ty.to_string(),
    };
    let signature = &types.functions[id as usize];
    let mut params = Vec::new();
    if let Some(receiver) = signature.receiver {
        params.push(format!(
            "this: {}",
            describe_depth(receiver, types, depth + 1)
        ));
    }
    let fixed = signature.shape.fixed_count();
    for (index, param) in signature.params.iter().enumerate() {
        let text = describe_depth(*param, types, depth + 1);
        params.push(if index >= fixed && signature.shape.rest {
            format!("...p{index}: {text}")
        } else if signature
            .shape
            .optional
            .get(index)
            .copied()
            .unwrap_or(false)
        {
            format!("p{index}?: {text}")
        } else {
            format!("p{index}: {text}")
        });
    }
    format!(
        "({}) => {}",
        params.join(", "),
        describe_depth(signature.result, types, depth + 1)
    )
}

impl Lower<'_> {
    /// The two types of a mismatch diagnostic, with function signatures
    /// spelled out under v37 when both are function types.
    pub(super) fn mismatch(&self, expected: Type, actual: Type) -> (String, String) {
        // V38 names both object types and why the actual one is not a
        // structural subtype of the expected one.
        if self.types.profile.structural_subtyping()
            && let Some(reasons) = subtyping::reasons(expected, actual, self.types)
        {
            return (
                subtyping::describe(expected, self.types),
                format!("{} ({reasons})", subtyping::describe(actual, self.types)),
            );
        }
        if self.types.profile.language_core()
            && matches!(expected, Type::Function(_))
            && matches!(actual, Type::Function(_))
        {
            (describe(expected, self.types), describe(actual, self.types))
        } else {
            (expected.to_string(), actual.to_string())
        }
    }
    /// Convert `value` to the `void`-result function type `expected` when its
    /// parameters match exactly; any other value is returned unchanged.
    pub(super) fn void_function(&mut self, value: Expr, expected: Option<Type>) -> Result<Expr> {
        let Type::Function(source) = value.ty else {
            return Ok(value);
        };
        if !self.types.profile.language_core() {
            return Ok(value);
        }
        // A union admits at most one function member (v36): convert to it,
        // and the union widening follows.
        let target = match expected {
            Some(Type::Function(target)) => target,
            Some(union) if union.is_union() => {
                match unions::members(union, self.types)
                    .into_iter()
                    .find(|member| matches!(member, Type::Function(_)))
                {
                    Some(Type::Function(target)) => target,
                    _ => return Ok(value),
                }
            }
            _ => return Ok(value),
        };
        if target == source {
            return Ok(value);
        }
        let expected = Some(Type::Function(target));
        let wanted = self.types.functions[target as usize].clone();
        let actual = self.types.functions[source as usize].clone();
        if wanted.result != Type::Void
            || actual.result == Type::Void
            || wanted.params != actual.params
            || wanted.shape != actual.shape
            || wanted.shape.extended()
            || wanted.receiver.is_some()
            || actual.receiver.is_some()
        {
            return Ok(value);
        }
        let span = value.span;
        let name = format!("{CONVERTED}{}", self.next);
        self.declare(&name, Some(value.ty), true, false)?;
        let binding = self.binding(&name)?;
        let ident = |text: &str| ast::Ident::new(text.into(), span, Default::default());
        let params: Vec<String> = (0..wanted.params.len())
            .map(|index| format!("{name}p{index}"))
            .collect();
        let call = ast::Expr::Call(ast::CallExpr {
            span,
            callee: ast::Callee::Expr(Box::new(ast::Expr::Ident(ident(&name)))),
            args: params
                .iter()
                .map(|param| ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(ast::Expr::Ident(ident(param))),
                })
                .collect(),
            ..Default::default()
        });
        let arrow = ast::ArrowExpr {
            span,
            params: params
                .iter()
                .map(|param| {
                    ast::Pat::Ident(ast::BindingIdent {
                        id: ident(param),
                        type_ann: None,
                    })
                })
                .collect(),
            body: Box::new(ast::ArrowFunctionBody::FunctionBody(ast::FunctionBody {
                span,
                stmts: vec![ast::Stmt::Expr(ast::ExprStmt {
                    span,
                    expr: Box::new(call),
                })],
            })),
            ..Default::default()
        };
        let wrapper = self.arrow(&arrow, expected)?;
        let ty = wrapper.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![S::Init(binding.slot, value), S::Init(output, wrapper)],
            },
        })
    }
}
