//! Source construct names for suspension diagnostics. Lowering encodes intrinsic
//! evaluation order in value blocks; the construct at a block's span decides
//! whether normalization may split it or must reject it by name.
use super::*;
use swc_ecma_visit::{Visit, VisitWith};

pub(super) struct Construct {
    pub name: String,
    /// Assignment preparation (receiver, key, old value, store) and the
    /// branches of `&&`, `||`, `??` and `?:` are split into ordinary statements
    /// and explicit branches; every other intrinsic block stays rejected.
    pub splittable: bool,
}

pub(super) struct Namer<'a> {
    module: Option<&'a ast::Module>,
    profile: varta_common::language::LanguageProfile,
}
impl<'a> Namer<'a> {
    pub(super) fn new(
        graph: &'a crate::modules::Graph,
        source: &Source,
        profile: varta_common::language::LanguageProfile,
    ) -> Self {
        let module = graph
            .units
            .values()
            .find(|unit| std::ptr::eq(&*unit.source.file, &*source.file))
            .map(|unit| &unit.ast);
        Self { module, profile }
    }
    pub(super) fn describe(&self, span: swc_common::Span) -> Construct {
        let mut find = Find {
            target: span,
            found: None,
            profile: self.profile,
        };
        if let Some(module) = self.module {
            module.visit_with(&mut find);
        }
        find.found.unwrap_or_else(|| Construct {
            name: "intrinsic expression".into(),
            splittable: false,
        })
    }
}

struct Find {
    target: swc_common::Span,
    found: Option<Construct>,
    profile: varta_common::language::LanguageProfile,
}
impl Visit for Find {
    fn visit_expr(&mut self, expr: &ast::Expr) {
        let span = expr.span();
        if span.lo > self.target.lo || self.target.hi > span.hi {
            return;
        }
        if span == self.target && !matches!(expr, ast::Expr::Paren(_)) {
            self.found = Some(describe(expr, self.profile));
        }
        expr.visit_children_with(self);
    }
}

fn describe(expr: &ast::Expr, profile: varta_common::language::LanguageProfile) -> Construct {
    let (name, splittable) = match expr {
        ast::Expr::Assign(assign) => (
            match assign.op {
                ast::AssignOp::Assign => "assignment",
                ast::AssignOp::AndAssign
                | ast::AssignOp::OrAssign
                | ast::AssignOp::NullishAssign => "logical assignment",
                _ => "compound assignment",
            }
            .to_owned(),
            true,
        ),
        ast::Expr::Update(_) => ("update expression".to_owned(), true),
        ast::Expr::Bin(binary) => (
            format!("`{}` expression", binary.op.as_str()),
            matches!(
                binary.op,
                ast::BinaryOp::LogicalAnd
                    | ast::BinaryOp::LogicalOr
                    | ast::BinaryOp::NullishCoalescing
            ),
        ),
        ast::Expr::Unary(unary) => (format!("`{}` expression", unary.op.as_str()), false),
        ast::Expr::Cond(_) => ("conditional expression".to_owned(), true),
        // A v36 async generator `yield` awaits, settles the request and reads
        // the next one; a synchronous `yield` is a bare suspension point.
        ast::Expr::Yield(_) => ("yield expression".to_owned(), true),
        // A checked `x as T` (v36) evaluates its operand first, then tests
        // it; earlier profiles keep rejecting it as an intrinsic expression.
        ast::Expr::TsAs(_) if profile.async_generators() => ("type assertion".to_owned(), true),
        ast::Expr::Call(call) if call.args.iter().any(|arg| arg.spread.is_some()) => {
            ("call with spread arguments".to_owned(), false)
        }
        ast::Expr::Call(ast::CallExpr {
            callee: ast::Callee::Expr(callee),
            ..
        }) => (
            match (path(callee), &**callee) {
                (Some(name), _) => format!("call to {name}"),
                (None, ast::Expr::Member(member)) => match &member.prop {
                    ast::MemberProp::Ident(method) => format!("`{}` method call", method.sym),
                    _ => "computed method call".to_owned(),
                },
                (None, _) => "call".to_owned(),
            },
            false,
        ),
        ast::Expr::Call(_) => ("call".to_owned(), false),
        ast::Expr::New(_) => ("new expression".to_owned(), false),
        ast::Expr::Tpl(_) => ("template literal".to_owned(), false),
        ast::Expr::TaggedTpl(_) => ("tagged template".to_owned(), false),
        ast::Expr::Member(_) | ast::Expr::SuperProp(_) => ("property access".to_owned(), false),
        ast::Expr::OptChain(_) => ("optional chain".to_owned(), false),
        ast::Expr::Object(_) => ("object literal".to_owned(), false),
        ast::Expr::Array(_) => ("array literal".to_owned(), false),
        ast::Expr::Seq(_) => ("comma expression".to_owned(), false),
        ast::Expr::Arrow(_) | ast::Expr::Fn(_) => ("function expression".to_owned(), false),
        _ => ("intrinsic expression".to_owned(), false),
    };
    Construct { name, splittable }
}

/// `a.b.c` for identifier/member chains, so diagnostics can name intrinsics.
fn path(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Ident(id) => Some(id.sym.to_string()),
        ast::Expr::Member(member) => {
            let ast::MemberProp::Ident(property) = &member.prop else {
                return None;
            };
            Some(format!("{}.{}", path(&member.obj)?, property.sym))
        }
        ast::Expr::Paren(paren) => path(&paren.expr),
        _ => None,
    }
}
