//! Names bound by local scopes, so rewriting never touches a shadowing
//! binding. A function scope binds its parameters and every `var` and
//! nested declaration it hoists; a block binds its lexical declarations.
use std::collections::BTreeSet;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitWith};

pub(super) type Names = BTreeSet<String>;

/// Every name a binding pattern binds.
pub(super) fn pattern(pat: &ast::Pat, names: &mut Names) {
    match pat {
        ast::Pat::Ident(binding) => {
            names.insert(binding.id.sym.to_string());
        }
        ast::Pat::Array(array) => {
            for element in array.elems.iter().flatten() {
                pattern(element, names);
            }
        }
        ast::Pat::Rest(rest) => pattern(&rest.arg, names),
        ast::Pat::Object(object) => {
            for property in &object.props {
                match property {
                    ast::ObjectPatProp::KeyValue(pair) => pattern(&pair.value, names),
                    ast::ObjectPatProp::Assign(assign) => {
                        names.insert(assign.key.sym.to_string());
                    }
                    ast::ObjectPatProp::Rest(rest) => pattern(&rest.arg, names),
                }
            }
        }
        ast::Pat::Assign(assign) => pattern(&assign.left, names),
        ast::Pat::Invalid(_) | ast::Pat::Expr(_) => {}
    }
}

/// Lexical declarations directly in a statement list.
pub(super) fn block(statements: &[ast::Stmt], names: &mut Names) {
    for statement in statements {
        if let ast::Stmt::Decl(decl) = statement {
            declaration(decl, names);
        }
    }
}

fn declaration(decl: &ast::Decl, names: &mut Names) {
    match decl {
        ast::Decl::Var(var) => {
            for declarator in &var.decls {
                pattern(&declarator.name, names);
            }
        }
        ast::Decl::Fn(function) => {
            names.insert(function.ident.sym.to_string());
        }
        ast::Decl::Class(class) => {
            names.insert(class.ident.sym.to_string());
        }
        ast::Decl::TsEnum(enumeration) => {
            names.insert(enumeration.id.sym.to_string());
        }
        _ => {}
    }
}

/// The names a node references as values without binding them itself:
/// identifiers in expression position and shorthand properties that no
/// scope inside the node binds.
pub(super) fn free_references(node: &impl VisitWith<Free>) -> Names {
    let mut free = Free {
        scopes: Vec::new(),
        found: Names::new(),
    };
    node.visit_with(&mut free);
    free.found
}

pub(super) struct Free {
    scopes: Vec<Names>,
    found: Names,
}
impl Free {
    fn reference(&mut self, name: &str) {
        if !self.scopes.iter().any(|scope| scope.contains(name)) {
            self.found.insert(name.to_owned());
        }
    }
    fn scoped(&mut self, names: Names, visit: impl FnOnce(&mut Self)) {
        self.scopes.push(names);
        visit(self);
        self.scopes.pop();
    }
}
impl Visit for Free {
    fn visit_expr(&mut self, expr: &ast::Expr) {
        if let ast::Expr::Ident(id) = expr {
            self.reference(id.sym.as_ref());
        }
        expr.visit_children_with(self);
    }
    fn visit_prop(&mut self, prop: &ast::Prop) {
        if let ast::Prop::Shorthand(id) = prop {
            self.reference(id.sym.as_ref());
        }
        prop.visit_children_with(self);
    }
    fn visit_function(&mut self, function: &ast::Function) {
        let mut names = Names::new();
        for param in &function.params {
            pattern(&param.pat, &mut names);
        }
        if let Some(body) = &function.body {
            hoisted(&body.stmts, &mut names);
            block(&body.stmts, &mut names);
        }
        self.scoped(names, |this| function.visit_children_with(this));
    }
    fn visit_constructor(&mut self, constructor: &ast::Constructor) {
        let mut names = Names::new();
        for param in &constructor.params {
            match param {
                ast::ParamOrTsParamProp::Param(param) => pattern(&param.pat, &mut names),
                ast::ParamOrTsParamProp::TsParamProp(prop) => match &prop.param {
                    ast::TsParamPropParam::Ident(id) => {
                        names.insert(id.id.sym.to_string());
                    }
                    ast::TsParamPropParam::Assign(assign) => pattern(&assign.left, &mut names),
                },
            }
        }
        if let Some(body) = &constructor.body {
            hoisted(&body.stmts, &mut names);
            block(&body.stmts, &mut names);
        }
        self.scoped(names, |this| constructor.visit_children_with(this));
    }
    fn visit_arrow_expr(&mut self, arrow: &ast::ArrowExpr) {
        let mut names = Names::new();
        for param in &arrow.params {
            pattern(param, &mut names);
        }
        if let ast::ArrowFunctionBody::FunctionBody(body) = &*arrow.body {
            hoisted(&body.stmts, &mut names);
            block(&body.stmts, &mut names);
        }
        self.scoped(names, |this| arrow.visit_children_with(this));
    }
    fn visit_block_stmt(&mut self, statement: &ast::BlockStmt) {
        let mut names = Names::new();
        block(&statement.stmts, &mut names);
        self.scoped(names, |this| statement.visit_children_with(this));
    }
    fn visit_for_stmt(&mut self, statement: &ast::ForStmt) {
        let mut names = Names::new();
        if let Some(ast::VarDeclOrExpr::VarDecl(var)) = &statement.init {
            for declarator in &var.decls {
                pattern(&declarator.name, &mut names);
            }
        }
        self.scoped(names, |this| statement.visit_children_with(this));
    }
    fn visit_for_of_stmt(&mut self, statement: &ast::ForOfStmt) {
        let mut names = Names::new();
        if let ast::ForHead::VarDecl(var) = &statement.left {
            for declarator in &var.decls {
                pattern(&declarator.name, &mut names);
            }
        }
        self.scoped(names, |this| statement.visit_children_with(this));
    }
    fn visit_for_in_stmt(&mut self, statement: &ast::ForInStmt) {
        let mut names = Names::new();
        if let ast::ForHead::VarDecl(var) = &statement.left {
            for declarator in &var.decls {
                pattern(&declarator.name, &mut names);
            }
        }
        self.scoped(names, |this| statement.visit_children_with(this));
    }
    fn visit_catch_clause(&mut self, clause: &ast::CatchClause) {
        let mut names = Names::new();
        if let Some(param) = &clause.param {
            pattern(param, &mut names);
        }
        self.scoped(names, |this| clause.visit_children_with(this));
    }
}

/// `var` declarations a function body hoists, outside nested functions.
pub(super) fn hoisted(body: &[ast::Stmt], names: &mut Names) {
    struct Vars<'a>(&'a mut Names);
    impl Visit for Vars<'_> {
        fn visit_var_decl(&mut self, var: &ast::VarDecl) {
            if var.kind == ast::VarDeclKind::Var {
                for declarator in &var.decls {
                    pattern(&declarator.name, self.0);
                }
            }
            var.visit_children_with(self);
        }
        fn visit_function(&mut self, _: &ast::Function) {}
        fn visit_arrow_expr(&mut self, _: &ast::ArrowExpr) {}
        fn visit_class(&mut self, _: &ast::Class) {}
    }
    for statement in body {
        statement.visit_with(&mut Vars(names));
    }
}
