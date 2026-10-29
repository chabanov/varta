//! Test-only lowering hook: while enabled on the current thread, a call to
//! `__varta_suspend(value[, witness])` lowers to a suspension point. The literal
//! witness selects the resume input type (number by default). Compiled only
//! for unit tests or the test-only `suspension-test-hook` feature, so no
//! profile admits suspension syntax.
use super::*;
use std::cell::Cell;

pub(crate) const NAME: &str = "__varta_suspend";
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
}
/// Whether the hook is enabled on this thread.
pub(crate) fn enabled() -> bool {
    ENABLED.with(Cell::get)
}
/// Run `f` with the hook enabled on this thread.
pub(crate) fn with<T>(f: impl FnOnce() -> T) -> T {
    ENABLED.with(|enabled| enabled.set(true));
    let result = f();
    ENABLED.with(|enabled| enabled.set(false));
    result
}
impl Lower<'_> {
    pub(in crate::lower) fn suspension_hook(
        &mut self,
        call: &ast::CallExpr,
    ) -> Result<Option<Expr>> {
        let ast::Callee::Expr(callee) = &call.callee else {
            return Ok(None);
        };
        let ast::Expr::Ident(name) = &**callee else {
            return Ok(None);
        };
        if name.sym != NAME || !enabled() {
            return Ok(None);
        }
        let (value, witness) = match call.args.as_slice() {
            [value] => (value, None),
            [value, witness] => (value, Some(&*witness.expr)),
            _ => {
                return Err(invalid(
                    "suspension hook takes a value and an optional witness",
                ));
            }
        };
        let input = match witness {
            None | Some(ast::Expr::Lit(ast::Lit::Num(_))) => self.types.number(),
            Some(ast::Expr::Lit(ast::Lit::Bool(_))) => Type::Boolean,
            Some(ast::Expr::Ident(id)) if id.sym == "undefined" => Type::Undefined,
            Some(ast::Expr::Lit(ast::Lit::Str(_))) => Type::String,
            Some(_) => return Err(invalid("suspension hook witness must be a literal")),
        };
        let value = self.expr(&value.expr)?;
        Ok(Some(Expr {
            span: call.span,
            ty: input,
            kind: E::Suspend {
                value: Box::new(value),
                state: 0,
            },
        }))
    }
}
