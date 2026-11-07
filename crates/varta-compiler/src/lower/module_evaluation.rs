//! Module evaluation with effects (v38 `module_evaluation`). ECMAScript
//! evaluates a module body once per instance, in source order; a module
//! initializer may therefore call functions and construct objects, and
//! class static fields and static blocks run where the class is defined.
//! Initializers run in dependency order before `main`, before each HTTP v2
//! request (a request's fresh Store is its own instance) and before each
//! tool invocation.
//!
//! A binding read before its initialization throws `ReferenceError` in
//! JavaScript; here such a read is refused at compile time whenever an
//! initializer can reach it through the module's own code. Reachability
//! over-approximates calls: naming a function or binding reaches its whole
//! body, naming a class reaches its constructor, instance fields and
//! instance methods, and `C.m` reaches the static method `m`. Other modules
//! cannot reach this module's bindings early: imports are acyclic and
//! initialized first.
use super::*;
use std::collections::BTreeSet;
use swc_ecma_visit::{Visit, VisitWith};

/// The identifiers a node mentions, and its `C.m` member paths.
#[derive(Default)]
pub(super) struct References {
    names: BTreeSet<String>,
}
impl Visit for References {
    fn visit_ident(&mut self, id: &ast::Ident) {
        self.names.insert(id.sym.to_string());
    }
    fn visit_member_expr(&mut self, member: &ast::MemberExpr) {
        if let (ast::Expr::Ident(class), ast::MemberProp::Ident(name)) =
            (&*member.obj, &member.prop)
        {
            self.names
                .insert(static_node(class.sym.as_ref(), name.sym.as_ref()));
        }
        member.visit_children_with(self);
    }
}
fn references(node: &impl VisitWith<References>) -> BTreeSet<String> {
    let mut found = References::default();
    node.visit_with(&mut found);
    found.names
}
/// The reachability node of a static member `C.m` (no identifier contains
/// a space).
fn static_node(class: &str, member: &str) -> String {
    format!("{class} {member}")
}

/// Whether an initializer runs program code while it is evaluated (a call,
/// a `new` or a tagged template outside a function value).
pub(super) fn effectful(value: &ast::Expr) -> bool {
    struct Calls(bool);
    impl Visit for Calls {
        fn visit_call_expr(&mut self, _: &ast::CallExpr) {
            self.0 = true;
        }
        fn visit_new_expr(&mut self, _: &ast::NewExpr) {
            self.0 = true;
        }
        fn visit_tagged_tpl(&mut self, _: &ast::TaggedTpl) {
            self.0 = true;
        }
        // A generated expando initializer (v38) defines a property.
        fn visit_assign_expr(&mut self, assign: &ast::AssignExpr) {
            self.0 |= crate::desugar::expando_target(&ast::Expr::Assign(assign.clone())).is_some();
            assign.visit_children_with(self);
        }
        // A function value's body runs later, when it is called.
        fn visit_arrow_expr(&mut self, _: &ast::ArrowExpr) {}
        fn visit_function(&mut self, _: &ast::Function) {}
    }
    let mut calls = Calls(false);
    value.visit_with(&mut calls);
    calls.0
}

/// Whether an initializer awaits outside a function value (top-level
/// `await`, admitted only under `expression_forms`; see
/// `top_level_await`).
pub(super) fn awaits(value: &ast::Expr) -> bool {
    struct Awaits(bool);
    impl Visit for Awaits {
        fn visit_await_expr(&mut self, _: &ast::AwaitExpr) {
            self.0 = true;
        }
        fn visit_arrow_expr(&mut self, _: &ast::ArrowExpr) {}
        fn visit_function(&mut self, _: &ast::Function) {}
    }
    let mut found = Awaits(false);
    value.visit_with(&mut found);
    found.0
}

/// The code of the module's declarations, by reachability node.
fn bodies(module: &ast::Module) -> BTreeMap<String, BTreeSet<String>> {
    let mut bodies: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for item in &module.body {
        let decl = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => decl,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
            _ => continue,
        };
        match decl {
            ast::Decl::Fn(function) => {
                bodies
                    .entry(function.ident.sym.to_string())
                    .or_default()
                    .extend(references(&*function.function));
            }
            ast::Decl::Class(class) => {
                let name = class.ident.sym.to_string();
                let mut instance = references(&class.class.super_class);
                for member in &class.class.body {
                    let (key, node) = match member {
                        ast::ClassMember::Method(m) if m.is_static => {
                            let key = match &m.key {
                                ast::PropName::Ident(id) => id.sym.to_string(),
                                _ => String::new(),
                            };
                            (Some(key), references(&*m.function))
                        }
                        member => (None, references(member)),
                    };
                    match key {
                        Some(key) => bodies
                            .entry(static_node(&name, &key))
                            .or_default()
                            .extend(node),
                        None => instance.extend(node),
                    }
                }
                bodies.entry(name).or_default().extend(instance);
            }
            ast::Decl::Var(var) => {
                for declarator in &var.decls {
                    if let (ast::Pat::Ident(binding), Some(init)) =
                        (&declarator.name, declarator.init.as_deref())
                    {
                        bodies
                            .entry(binding.id.sym.to_string())
                            .or_default()
                            .extend(references(init));
                    }
                }
            }
            _ => {}
        }
    }
    bodies
}

/// Every identifier the code an effectful initializer can run mentions.
pub(super) fn reachable(module: &ast::Module, value: &ast::Expr) -> BTreeSet<String> {
    let bodies = bodies(module);
    let mut reached = references(value);
    let mut pending: Vec<String> = reached.iter().cloned().collect();
    while let Some(name) = pending.pop() {
        for next in bodies.get(&name).into_iter().flatten() {
            if reached.insert(next.clone()) {
                pending.push(next.clone());
            }
        }
    }
    reached
}
