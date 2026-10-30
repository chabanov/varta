//! Function-valued members of the v38 typed global object (`web_bytes`).
//!
//! `globalThis.fetch` (and the other names of [`FUNCTION_GLOBALS`]) is the
//! global function itself, so the scanner rewrites the member expression into
//! the identifier before implicit prelude imports are computed: the module
//! then imports and lowers the function exactly as if it had written the bare
//! name. The rewrite skips modules that bind `globalThis` or the member's name
//! themselves, whose member read stays a read of the `GlobalObject` class (and
//! a compile error for a member it does not have). Every other member of the
//! global object (`crypto`, `process`, `globalThis`) is a getter of the global
//! prelude's `GlobalObject`.
use super::engine::GLOBAL_THIS;
use std::collections::BTreeSet;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitMut, VisitMutWith, VisitWith};

/// Global functions a program may read through `globalThis`.
pub(crate) const FUNCTION_GLOBALS: [&str; 3] = ["fetch", "encodeURIComponent", "encodeURI"];

/// Every name a module binds, at any depth.
fn bindings(module: &ast::Module) -> BTreeSet<String> {
    #[derive(Default)]
    struct Names(BTreeSet<String>);
    impl Visit for Names {
        fn visit_binding_ident(&mut self, binding: &ast::BindingIdent) {
            self.0.insert(binding.id.sym.to_string());
        }
        fn visit_fn_decl(&mut self, function: &ast::FnDecl) {
            self.0.insert(function.ident.sym.to_string());
            function.visit_children_with(self);
        }
        fn visit_class_decl(&mut self, class: &ast::ClassDecl) {
            self.0.insert(class.ident.sym.to_string());
            class.visit_children_with(self);
        }
        fn visit_import_named_specifier(&mut self, import: &ast::ImportNamedSpecifier) {
            self.0.insert(import.local.sym.to_string());
        }
    }
    let mut names = Names::default();
    module.visit_with(&mut names);
    names.0
}

struct Rewrite {
    bound: BTreeSet<String>,
}
impl VisitMut for Rewrite {
    fn visit_mut_expr(&mut self, expression: &mut ast::Expr) {
        if let ast::Expr::Member(member) = expression
            && let ast::Expr::Ident(object) = &*member.obj
            && object.sym == GLOBAL_THIS
            && let ast::MemberProp::Ident(property) = &member.prop
            && FUNCTION_GLOBALS.contains(&property.sym.as_ref())
            && !self.bound.contains(property.sym.as_ref())
        {
            *expression = ast::Expr::Ident(ast::Ident::new(
                property.sym.clone(),
                member.span,
                Default::default(),
            ));
            return;
        }
        expression.visit_mut_children_with(self);
    }
}

/// Rewrite `globalThis.NAME` of a function global into `NAME`.
pub(super) fn rewrite(module: &mut ast::Module) {
    let bound = bindings(module);
    if bound.contains(GLOBAL_THIS) {
        return;
    }
    module.visit_mut_with(&mut Rewrite { bound });
}
