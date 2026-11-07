//! The temporal dead zone of module-level classes (v38 `module_evaluation`,
//! coordinator decision of 2026-11-07).
//!
//! A class declaration's binding is initialized after the class is defined,
//! its static elements included; code outside the class body that reaches
//! the class earlier throws `ReferenceError` (inside the body, the class's
//! own scope binding is initialized before its static elements run).
//! `desugar::statics` declares the module constant `class_binding(C)`
//! (`true`) after the class's static elements, and `module_constants::check`
//! flags it, like any binding, when code may reach the class before that
//! position. Every reference to such a class outside its body then tests the
//! flag first: `new C(…)`, `C.m(…)`, `C.x` (a static field's module
//! variable, read or written), `C` as a value and `x instanceof C`, where
//! JavaScript evaluates `x` first: its test runs after the left operand
//! (`class_instance_of`, through a hidden binding). Code lexically inside the class, its
//! members and its static elements (lowered as module initializers with
//! `Classes::static_owner` set), takes no test.
use super::*;

/// The hidden binding of an `instanceof` left operand.
const INSTANCE_BINDING: &str = "#instance";

impl Lower<'_> {
    /// `value`, the lowered `expression`, after the flag test of the class
    /// it references outside the class body, when code may reach that
    /// class early. Kept out of `expr_lowered` so its frame stays small.
    #[inline(never)]
    pub(super) fn class_binding_guard(
        &mut self,
        expression: &ast::Expr,
        value: Expr,
    ) -> Result<Expr> {
        Ok(match self.class_binding_check(expression)? {
            Some(check) => self.class_binding_guarded(check, value),
            None => value,
        })
    }
    /// The flag test before `expression`, a reference to a module-level
    /// class outside its body whose binding code may reach early.
    fn class_binding_check(&mut self, expression: &ast::Expr) -> Result<Option<S>> {
        if !self.types.profile.module_evaluation() || self.types.classes.class_flags.is_empty() {
            return Ok(None);
        }
        let Some(class) = self.referenced_class(expression) else {
            return Ok(None);
        };
        self.class_flag_check(class, expression.span())
    }
    /// The flag test of module-level `class` referenced at `span` outside
    /// its body, when code may reach that class early.
    pub(super) fn class_flag_check(
        &mut self,
        class: u32,
        span: swc_common::Span,
    ) -> Result<Option<S>> {
        if !self.types.profile.module_evaluation() || self.types.classes.class_flags.is_empty() {
            return Ok(None);
        }
        let classes = &self.types.classes;
        if classes
            .context
            .is_some_and(|context| context.class == class)
            || classes.static_owner == Some(class)
        {
            return Ok(None);
        }
        let Some(flag) = classes.class_flags.get(&class).copied() else {
            return Ok(None);
        };
        let shown = crate::desugar::display_name(&classes.get(class).name).to_owned();
        self.module_flag_check(flag, &shown, span).map(Some)
    }
    /// `x instanceof C` for a class `C` code may reach early: `x` is
    /// evaluated first into a hidden binding, then `C` is tested, then
    /// `test` (`class_instance_of` without the flag test) runs on the
    /// binding (ECMA-262 §13.10.1: the left operand before the right).
    pub(super) fn class_instance_after_left(
        &mut self,
        binary: &ast::BinExpr,
        check: S,
        test: impl FnOnce(&mut Self, &ast::BinExpr) -> Result<Expr>,
    ) -> Result<Expr> {
        let span = binary.span;
        let value = self.expr(&binary.left)?;
        let ty = value.ty;
        let name = format!("{INSTANCE_BINDING}{}", self.next);
        self.scopes.push(BTreeMap::new());
        let lowered = self.declare(&name, Some(ty), true, false).and_then(|()| {
            let slot = self.binding(&name)?.slot;
            let left = ast::Expr::Ident(ast::Ident {
                span: binary.left.span(),
                ctxt: Default::default(),
                sym: name.as_str().into(),
                optional: false,
            });
            let rebound = ast::BinExpr {
                left: Box::new(left),
                ..binary.clone()
            };
            Ok((slot, test(self, &rebound)?))
        });
        self.scopes.pop();
        let (slot, result) = lowered?;
        let output = self.hidden(result.ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: result.ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![slot, output],
                body: vec![S::Init(slot, value), check, S::Init(output, result)],
            },
        })
    }
    /// `value` after the flag test `check`.
    fn class_binding_guarded(&mut self, check: S, value: Expr) -> Expr {
        let (span, ty) = (value.span, value.ty);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![check, S::Init(output, value)],
            },
        }
    }
    /// The module-level class `expression` references by name.
    fn referenced_class(&self, expression: &ast::Expr) -> Option<u32> {
        let named = |expr: &ast::Expr| {
            self.class_callee(expr)
                .filter(|class| !self.types.classes.get(*class).prelude)
        };
        let element = |name: &str| self.types.classes.static_elements.get(name).copied();
        match expression {
            ast::Expr::Ident(id) => named(expression).or_else(|| element(id.sym.as_ref())),
            ast::Expr::New(new) => named(&new.callee),
            ast::Expr::Call(call) => match &call.callee {
                ast::Callee::Expr(callee) => match stmt::unparen(callee) {
                    ast::Expr::Member(member) => named(&member.obj),
                    _ => None,
                },
                _ => None,
            },
            ast::Expr::Member(member) => named(&member.obj),
            ast::Expr::Assign(assign) => match &assign.left {
                ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(id)) => {
                    element(id.id.sym.as_ref())
                }
                _ => None,
            },
            ast::Expr::Update(update) => match &*update.arg {
                ast::Expr::Ident(id) => element(id.sym.as_ref()),
                _ => None,
            },
            _ => None,
        }
    }
}
