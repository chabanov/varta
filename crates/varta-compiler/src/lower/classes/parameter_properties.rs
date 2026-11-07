//! Constructor parameter properties (v38 `structural_subtyping`):
//! `constructor(private readonly x: T)` is a pre-layout AST rewrite into the
//! field declaration `private readonly x: T;`, the plain parameter `x: T` and
//! the assignment `this.x = x;` at the start of the constructor body, after
//! the `super(...)` call of a derived class, in parameter order, as
//! TypeScript emits it.
use super::*;

/// A parameter property with a default value (v38 `declaration_forms`):
/// its binding, the default and the field initializer that gives the field
/// its type when it has no annotation (a literal default only, so evaluating
/// it for the field and again for the parameter cannot be observed).
fn defaulted(
    prop: &ast::TsParamProp,
    profile: varta_common::language::LanguageProfile,
    source: &Source,
) -> Result<(ast::BindingIdent, Box<ast::Expr>, Option<Box<ast::Expr>>)> {
    let refused = || {
        source.locate(
            unsupported("constructor parameter property with a default value or pattern"),
            prop.span,
        )
    };
    let ast::TsParamPropParam::Assign(assign) = &prop.param else {
        return Err(refused());
    };
    let ast::Pat::Ident(binding) = &*assign.left else {
        return Err(refused());
    };
    if !profile.declaration_forms() {
        return Err(refused());
    }
    let literal = matches!(
        &*assign.right,
        ast::Expr::Lit(ast::Lit::Num(_) | ast::Lit::Str(_) | ast::Lit::Bool(_))
    );
    if binding.type_ann.is_none() && !literal {
        return Err(source.locate(
            type_error(
                "a parameter property with a non-literal default value needs a type annotation",
            ),
            prop.span,
        ));
    }
    let initializer = (binding.type_ann.is_none()).then(|| assign.right.clone());
    Ok((binding.clone(), assign.right.clone(), initializer))
}

/// The class with its constructor parameter properties rewritten.
pub(super) fn rewrite(
    mut class: Box<ast::Class>,
    source: &Source,
    profile: varta_common::language::LanguageProfile,
) -> Result<Box<ast::Class>> {
    let mut fields = Vec::new();
    for member in &mut class.body {
        let ast::ClassMember::Constructor(constructor) = member else {
            continue;
        };
        let mut assignments = Vec::new();
        for param in &mut constructor.params {
            let ast::ParamOrTsParamProp::TsParamProp(prop) = param else {
                continue;
            };
            let (binding, default, initializer) = match &prop.param {
                ast::TsParamPropParam::Ident(binding) => (binding.clone(), None, None),
                ast::TsParamPropParam::Assign(_) => {
                    let (binding, default, initializer) = defaulted(prop, profile, source)?;
                    (binding, Some(default), initializer)
                }
            };
            let binding = &binding;
            if prop.is_override || !prop.decorators.is_empty() {
                return Err(source.locate(
                    unsupported("override or decorated constructor parameter property"),
                    prop.span,
                ));
            }
            let span = prop.span;
            let name = binding.id.clone();
            fields.push(ast::ClassMember::ClassProp(ast::ClassProp {
                span,
                key: ast::PropName::Ident(ast::IdentName::new(name.sym.clone(), name.span)),
                value: initializer,
                type_ann: binding.type_ann.clone(),
                is_static: false,
                decorators: Vec::new(),
                accessibility: prop.accessibility,
                is_abstract: false,
                is_optional: binding.id.optional,
                is_override: false,
                readonly: prop.readonly,
                declare: false,
                definite: false,
            }));
            assignments.push(ast::Stmt::Expr(ast::ExprStmt {
                span,
                expr: Box::new(ast::Expr::Assign(ast::AssignExpr {
                    span,
                    op: ast::AssignOp::Assign,
                    left: ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(
                        ast::MemberExpr {
                            span,
                            obj: Box::new(ast::Expr::This(ast::ThisExpr { span })),
                            prop: ast::MemberProp::Ident(ast::IdentName::new(
                                name.sym.clone(),
                                name.span,
                            )),
                        },
                    )),
                    right: Box::new(ast::Expr::Ident(ast::Ident {
                        optional: false,
                        ..name.clone()
                    })),
                })),
            }));
            let pat = match default {
                Some(right) => ast::Pat::Assign(ast::AssignPat {
                    span,
                    left: Box::new(ast::Pat::Ident(binding.clone())),
                    right,
                }),
                None => ast::Pat::Ident(binding.clone()),
            };
            *param = ast::ParamOrTsParamProp::Param(ast::Param {
                span,
                decorators: Vec::new(),
                pat,
            });
        }
        if assignments.is_empty() {
            continue;
        }
        let Some(body) = &mut constructor.body else {
            return Err(source.locate(
                unsupported("constructor parameter property without a constructor body"),
                constructor.span,
            ));
        };
        // After `super(...)` in a derived class, else first.
        let at = body
            .stmts
            .iter()
            .position(|statement| {
                matches!(statement, ast::Stmt::Expr(ast::ExprStmt { expr, .. })
                if matches!(&**expr, ast::Expr::Call(ast::CallExpr {
                    callee: ast::Callee::Super(_),
                    ..
                })))
            })
            .map_or(0, |index| index + 1);
        body.stmts.splice(at..at, assignments);
    }
    // Fields precede the members, so they take the layout's declared order.
    fields.append(&mut class.body);
    class.body = fields;
    Ok(class)
}
