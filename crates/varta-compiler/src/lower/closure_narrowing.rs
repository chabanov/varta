//! V38 (`closure_narrowing()`): TypeScript 5.4's preserved narrowing in
//! closures created after the last assignment. A `const` binding's
//! narrowing already reaches every closure (v24); under v38 a parameter or
//! `let` binding's narrowing reaches an arrow or function expression (a TSX
//! hole getter included) when
//!
//! - no nested function of the declaring function assigns the name, and
//! - the closure is created after the binding's last assignment in the
//!   declaring function, where an assignment inside a loop counts at the end
//!   of the outermost enclosing loop of that function (a later iteration
//!   runs it after the closure was created).
//!
//! Hoisted function declarations never take it, as in TypeScript. From its
//! creation on such a binding is never written again, so inside the
//! closure, and in closures nested in it, the narrowing is a fact that no
//! call can invalidate. Assignments are matched by name (a shadowing name
//! counts as an assignment), which only ever withholds narrowing.
use super::*;
use std::collections::BTreeSet;
use swc_ecma_visit::{Visit, VisitWith};

/// Where a name is assigned in one function body.
#[derive(Debug, Clone, Copy, Default)]
struct Site {
    /// The latest position (loop-extended) of an assignment in the body.
    last: u32,
    /// Assigned inside a nested function.
    nested: bool,
}

/// The assignment sites of one function body, by name.
#[derive(Debug, Default)]
pub(super) struct Assignments {
    sites: BTreeMap<String, Site>,
}
impl Assignments {
    /// Whether a binding named `name` is never written after a closure
    /// created at `at`.
    fn stable_at(&self, name: &str, at: swc_common::BytePos) -> bool {
        self.sites
            .get(name)
            .is_none_or(|site| !site.nested && site.last < at.0)
    }
}

/// Scan a function's parameters and body, visited by `visit`, for
/// assignments.
pub(super) fn scan(visit: impl FnOnce(&mut Scan)) -> std::rc::Rc<Assignments> {
    let mut scan = Scan::default();
    visit(&mut scan);
    std::rc::Rc::new(Assignments { sites: scan.sites })
}

#[derive(Default)]
pub(super) struct Scan {
    sites: BTreeMap<String, Site>,
    /// Nested function depth below the scanned body.
    functions: usize,
    /// End of the outermost loop of the scanned body around the visitor.
    outer_loop: Option<u32>,
}
impl Scan {
    fn assign(&mut self, name: &str, at: u32) {
        let site = self.sites.entry(name.to_owned()).or_default();
        if self.functions > 0 {
            site.nested = true;
        } else {
            site.last = site.last.max(self.outer_loop.map_or(at, |end| end.max(at)));
        }
    }
    fn targets(&mut self, pattern: &ast::Pat, at: u32) {
        let mut names = Names::default();
        pattern.visit_with(&mut names);
        for name in names.0 {
            self.assign(&name, at);
        }
    }
    fn looped(&mut self, end: u32, visit: impl FnOnce(&mut Self)) {
        let saved = self.outer_loop;
        if self.functions == 0 && saved.is_none() {
            self.outer_loop = Some(end);
        }
        visit(self);
        self.outer_loop = saved;
    }
    fn nested(&mut self, visit: impl FnOnce(&mut Self)) {
        self.functions += 1;
        let saved = self.outer_loop.take();
        visit(self);
        self.outer_loop = saved;
        self.functions -= 1;
    }
}

/// The names a destructuring target writes.
#[derive(Default)]
struct Names(BTreeSet<String>);
impl Visit for Names {
    fn visit_binding_ident(&mut self, binding: &ast::BindingIdent) {
        self.0.insert(binding.id.sym.to_string());
    }
    fn visit_expr(&mut self, _: &ast::Expr) {}
}

impl Visit for Scan {
    fn visit_assign_expr(&mut self, node: &ast::AssignExpr) {
        let at = node.span.lo.0;
        match &node.left {
            ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(id)) => {
                self.assign(id.id.sym.as_ref(), at);
            }
            ast::AssignTarget::Pat(pattern) => self.targets(&pattern.clone().into(), at),
            _ => {}
        }
        node.visit_children_with(self);
    }
    fn visit_update_expr(&mut self, node: &ast::UpdateExpr) {
        if let ast::Expr::Ident(id) = stmt::unparen(&node.arg) {
            self.assign(id.sym.as_ref(), node.span.lo.0);
        }
        node.visit_children_with(self);
    }
    fn visit_for_stmt(&mut self, node: &ast::ForStmt) {
        self.looped(node.span.hi.0, |scan| node.visit_children_with(scan));
    }
    fn visit_while_stmt(&mut self, node: &ast::WhileStmt) {
        self.looped(node.span.hi.0, |scan| node.visit_children_with(scan));
    }
    fn visit_do_while_stmt(&mut self, node: &ast::DoWhileStmt) {
        self.looped(node.span.hi.0, |scan| node.visit_children_with(scan));
    }
    fn visit_for_of_stmt(&mut self, node: &ast::ForOfStmt) {
        self.looped(node.span.hi.0, |scan| {
            if let ast::ForHead::Pat(pattern) = &node.left {
                scan.targets(pattern, node.span.lo.0);
            }
            node.visit_children_with(scan);
        });
    }
    fn visit_for_in_stmt(&mut self, node: &ast::ForInStmt) {
        self.looped(node.span.hi.0, |scan| {
            if let ast::ForHead::Pat(pattern) = &node.left {
                scan.targets(pattern, node.span.lo.0);
            }
            node.visit_children_with(scan);
        });
    }
    fn visit_function(&mut self, node: &ast::Function) {
        self.nested(|scan| node.visit_children_with(scan));
    }
    fn visit_arrow_expr(&mut self, node: &ast::ArrowExpr) {
        self.nested(|scan| node.visit_children_with(scan));
    }
    fn visit_class(&mut self, node: &ast::Class) {
        self.nested(|scan| node.visit_children_with(scan));
    }
    fn visit_ts_type(&mut self, _: &ast::TsType) {}
}

impl Lower<'_> {
    /// The identities whose narrowing a closure created at `at` may keep
    /// although they are mutable: those stable for this function's closure,
    /// and this function's parameters and `let` bindings never written after
    /// `at` (v38 `closure_narrowing()`).
    pub(super) fn stable_bindings(&self, at: swc_common::BytePos) -> BTreeSet<(usize, u32)> {
        let mut stable = self.core.stable.clone();
        let Some(assignments) = self
            .core
            .assignments
            .as_ref()
            .filter(|_| self.types.profile.closure_narrowing() && !self.core.hoisting)
        else {
            return stable;
        };
        for scope in &self.scopes {
            for (name, binding) in scope {
                if binding.mutable
                    && binding.identity.0 == self.index
                    && binding.global.is_none()
                    && assignments.stable_at(name, at)
                {
                    stable.insert(binding.identity);
                }
            }
        }
        stable
    }
    /// Whether a mutable binding is never written again in this function.
    pub(super) fn stable(&self, binding: &Binding) -> bool {
        self.core.stable.contains(&binding.identity)
    }
}
