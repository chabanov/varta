//! V38 contextually typed class-field functions (`expression_forms`).
//!
//! A class field without an annotation whose initializer is an arrow or
//! function expression takes the function type TypeScript infers for it:
//! the annotated parameter types (a default makes the parameter optional)
//! and the annotated return type, or, without one, the type of an
//! expression body whose operator fixes it (`*`, `-`, `<`, `!`, template
//! literals, literals, ...). A body whose type needs inference over other
//! declarations needs a return annotation and is refused by name.
use super::*;

/// The function type of a field initializer, if it is a function expression.
pub(super) fn field_type(value: &ast::Expr, types: &mut ir::ValueTypes) -> Result<Option<Type>> {
    let (params, return_type, body, generic) = match stmt::unparen(value) {
        ast::Expr::Arrow(arrow) => (
            arrow.params.iter().collect::<Vec<_>>(),
            arrow.return_type.clone(),
            match &*arrow.body {
                ast::ArrowFunctionBody::Expr(expr) => Some(&**expr),
                ast::ArrowFunctionBody::FunctionBody(_) => None,
            },
            arrow.type_params.is_some(),
        ),
        ast::Expr::Fn(function) => (
            function.function.params.iter().map(|p| &p.pat).collect(),
            function.function.return_type.clone(),
            None,
            function.function.type_params.is_some(),
        ),
        _ => return Ok(None),
    };
    if generic {
        return Err(unsupported(
            "generic function expression as an unannotated class field",
        ));
    }
    let mut lowered = Vec::with_capacity(params.len());
    for param in params {
        lowered.push(parameter(param)?);
    }
    let return_type = match return_type {
        Some(annotation) => annotation,
        None => {
            let keyword = body.and_then(evident).ok_or_else(|| {
                unsupported(
                    "class field function without a return type annotation whose body type is not evident from its operators",
                )
            })?;
            Box::new(ast::TsTypeAnn {
                span: value.span(),
                type_ann: Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
                    span: value.span(),
                    kind: keyword,
                })),
            })
        }
    };
    let function = ast::TsFnType {
        span: value.span(),
        params: lowered,
        type_params: None,
        type_ann: return_type,
    };
    functions::type_annotation(&function, types).map(Some)
}

pub(super) fn parameter(param: &ast::Pat) -> Result<ast::TsFnParam> {
    let missing = || unsupported("class field function parameter without a type annotation");
    Ok(match param {
        ast::Pat::Ident(binding) => {
            if binding.type_ann.is_none() {
                return Err(missing());
            }
            ast::TsFnParam::Ident(binding.clone())
        }
        ast::Pat::Assign(assign) => match &*assign.left {
            ast::Pat::Ident(binding) if binding.type_ann.is_some() => {
                let mut binding = binding.clone();
                binding.id.optional = true;
                ast::TsFnParam::Ident(binding)
            }
            _ => return Err(missing()),
        },
        ast::Pat::Rest(rest) if rest.type_ann.is_some() => ast::TsFnParam::Rest(rest.clone()),
        ast::Pat::Array(array) if array.type_ann.is_some() => ast::TsFnParam::Array(array.clone()),
        ast::Pat::Object(object) if object.type_ann.is_some() => {
            ast::TsFnParam::Object(object.clone())
        }
        _ => return Err(missing()),
    })
}

/// The primitive type an expression body has by its outermost operator.
fn evident(body: &ast::Expr) -> Option<ast::TsKeywordTypeKind> {
    use ast::TsKeywordTypeKind as K;
    Some(match stmt::unparen(body) {
        ast::Expr::Lit(ast::Lit::Num(_)) => K::TsNumberKeyword,
        ast::Expr::Lit(ast::Lit::Str(_)) | ast::Expr::Tpl(_) => K::TsStringKeyword,
        ast::Expr::Lit(ast::Lit::Bool(_)) => K::TsBooleanKeyword,
        // The primitive conversion functions (a shadowed name keeps its own
        // type through the ordinary check of the body).
        ast::Expr::Call(ast::CallExpr {
            callee: ast::Callee::Expr(callee),
            ..
        }) => match stmt::unparen(callee) {
            ast::Expr::Ident(id) if id.sym == "String" => K::TsStringKeyword,
            ast::Expr::Ident(id) if id.sym == "Number" => K::TsNumberKeyword,
            ast::Expr::Ident(id) if id.sym == "Boolean" => K::TsBooleanKeyword,
            _ => return None,
        },
        ast::Expr::Unary(unary) => match unary.op {
            ast::UnaryOp::Bang | ast::UnaryOp::Delete => K::TsBooleanKeyword,
            ast::UnaryOp::TypeOf => K::TsStringKeyword,
            ast::UnaryOp::Void => K::TsUndefinedKeyword,
            ast::UnaryOp::Minus | ast::UnaryOp::Plus | ast::UnaryOp::Tilde => K::TsNumberKeyword,
        },
        ast::Expr::Bin(binary) => {
            use ast::BinaryOp as B;
            match binary.op {
                B::EqEq
                | B::NotEq
                | B::EqEqEq
                | B::NotEqEq
                | B::Lt
                | B::LtEq
                | B::Gt
                | B::GtEq
                | B::In
                | B::InstanceOf => K::TsBooleanKeyword,
                B::Sub
                | B::Mul
                | B::Div
                | B::Mod
                | B::Exp
                | B::BitAnd
                | B::BitOr
                | B::BitXor
                | B::LShift
                | B::RShift
                | B::ZeroFillRShift => K::TsNumberKeyword,
                B::Add => {
                    let (left, right) = (evident(&binary.left), evident(&binary.right));
                    if left == Some(K::TsStringKeyword) || right == Some(K::TsStringKeyword) {
                        K::TsStringKeyword
                    } else {
                        return None;
                    }
                }
                _ => return None,
            }
        }
        _ => return None,
    })
}
