//! Constructor parameter properties (v38 `structural_subtyping`):
//! `constructor(private readonly x: T)` is a pre-layout AST rewrite into the
//! field declaration `private readonly x: T;`, the plain parameter `x: T` and
//! the assignment `this.x = x;` at the start of the constructor body, after
//! the `super(...)` call of a derived class, in parameter order, as
//! TypeScript emits it.
use super::*;

/// The class with its constructor parameter properties rewritten.
pub(super) fn rewrite(mut class: Box<ast::Class>, source: &Source) -> Result<Box<ast::Class>> {
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
            let ast::TsParamPropParam::Ident(binding) = &prop.param else {
                return Err(source.locate(
                    unsupported("constructor parameter property with a default value or pattern"),
                    prop.span,
                ));
            };
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
                value: None,
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
            *param = ast::ParamOrTsParamProp::Param(ast::Param {
                span,
                decorators: Vec::new(),
                pat: ast::Pat::Ident(binding.clone()),
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
