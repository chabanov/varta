//! Class expressions bound by `const` declarations.
//!
//! Classes are module declarations of a closed world, so a class expression
//! becomes one: `const X = class N { ... }` declares the class under the
//! binding name `X` at module level (or, when the name `N` differs or the
//! declaration is inside a function, under a generated name whose last
//! segment is the class's own name, which it shows at run time), and every
//! reference to `X` (and to `N` inside the class) becomes that name. A class
//! expression inside a function may not reference the function's local
//! bindings: it would capture them, and a module declaration cannot. A class
//! expression in any other position is rejected.
use super::rewrite::{self, Frame};
use super::scope::{self, Names};
use super::{Symbol, generated, unsupported};
use crate::CompilerError;
use crate::source::Source;
use swc_common::Span;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitMut, VisitMutWith, VisitWith};

pub(super) fn hoist(module: &mut ast::Module, source: &Source) -> Result<(), CompilerError> {
    if !contains_class_expression(module) {
        return Ok(());
    }
    let mut body = Vec::with_capacity(module.body.len());
    let mut counter = 0usize;
    // Module-level bindings renamed to their class's generated name.
    let mut renamed = Frame::new();
    for item in std::mem::take(&mut module.body) {
        // `const X = class {}` at module level.
        if let Some((exported, var)) = module_const(&item)
            && let Some((name, class)) = bound_class(var)
        {
            let span = var.span;
            let (ident, renames): (ast::Ident, Vec<(String, String)>) = match &class.ident {
                Some(own) if own.sym != name.sym => {
                    counter += 1;
                    let target = generated(&[&counter.to_string(), own.sym.as_ref()]);
                    (
                        ast::Ident::new_no_ctxt(target.as_str().into(), own.span),
                        vec![
                            (name.sym.to_string(), target.clone()),
                            (own.sym.to_string(), target),
                        ],
                    )
                }
                _ => (
                    ast::Ident::new_no_ctxt(name.sym.clone(), name.span),
                    Vec::new(),
                ),
            };
            let mut class = class.class.clone();
            let mut hoister = Hoister::new(source, &mut counter);
            class.visit_mut_with(&mut hoister);
            let hoisted = std::mem::take(&mut hoister.hoisted);
            hoister.finish()?;
            let mut decl = ast::Decl::Class(ast::ClassDecl {
                ident,
                declare: false,
                class,
            });
            if !renames.is_empty() {
                // `N` inside the class; `X` anywhere in the module (below).
                let frame: Frame = renames
                    .iter()
                    .map(|(from, to)| (from.clone(), Symbol::plain(to.clone())))
                    .collect();
                decl = rewrite::declaration(decl, source, &[&frame])?;
                let (from, to) = &renames[0];
                renamed.insert(from.clone(), Symbol::plain(to.clone()));
            }
            body.extend(hoisted);
            body.push(super::namespaces::item(decl, exported, span));
            continue;
        }
        let mut item = item;
        let mut hoister = Hoister::new(source, &mut counter);
        item.visit_mut_with(&mut hoister);
        let hoisted = std::mem::take(&mut hoister.hoisted);
        hoister.finish()?;
        body.extend(hoisted);
        body.push(item);
    }
    if !renamed.is_empty() {
        body = body
            .into_iter()
            .map(|item| rewrite::item(item, source, &[&renamed]))
            .collect::<Result<_, _>>()?;
    }
    module.body = body;
    Ok(())
}

/// Whether the module has a class expression or a class declared inside a
/// function.
fn contains_class_expression(module: &ast::Module) -> bool {
    struct Finder {
        found: bool,
        functions: usize,
    }
    impl Visit for Finder {
        fn visit_class_expr(&mut self, _: &ast::ClassExpr) {
            self.found = true;
        }
        fn visit_class_decl(&mut self, class: &ast::ClassDecl) {
            self.found |= self.functions > 0;
            class.visit_children_with(self);
        }
        fn visit_function(&mut self, function: &ast::Function) {
            self.functions += 1;
            function.visit_children_with(self);
            self.functions -= 1;
        }
        fn visit_arrow_expr(&mut self, arrow: &ast::ArrowExpr) {
            self.functions += 1;
            arrow.visit_children_with(self);
            self.functions -= 1;
        }
        fn visit_constructor(&mut self, constructor: &ast::Constructor) {
            self.functions += 1;
            constructor.visit_children_with(self);
            self.functions -= 1;
        }
    }
    let mut finder = Finder {
        found: false,
        functions: 0,
    };
    module.visit_with(&mut finder);
    finder.found
}

fn module_const(item: &ast::ModuleItem) -> Option<(bool, &ast::VarDecl)> {
    match item {
        ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(var))) => Some((false, var)),
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(ast::ExportDecl {
            decl: ast::Decl::Var(var),
            ..
        })) => Some((true, var)),
        _ => None,
    }
}

/// `const X = class ...` with a single plain binding.
fn bound_class(var: &ast::VarDecl) -> Option<(&ast::Ident, &ast::ClassExpr)> {
    if var.kind != ast::VarDeclKind::Const || var.declare {
        return None;
    }
    let [declarator] = var.decls.as_slice() else {
        return None;
    };
    let ast::Pat::Ident(binding) = &declarator.name else {
        return None;
    };
    if binding.type_ann.is_some() {
        return None;
    }
    match unparen(declarator.init.as_deref()?) {
        ast::Expr::Class(class) => Some((&binding.id, class)),
        _ => None,
    }
}

fn unparen(expr: &ast::Expr) -> &ast::Expr {
    match expr {
        ast::Expr::Paren(paren) => unparen(&paren.expr),
        expr => expr,
    }
}

/// Hoists the `const` class expressions of function bodies and rejects
/// every other class expression.
struct Hoister<'a> {
    source: &'a Source,
    counter: &'a mut usize,
    scopes: Vec<Names>,
    hoisted: Vec<ast::ModuleItem>,
    error: Option<CompilerError>,
}

impl<'a> Hoister<'a> {
    fn new(source: &'a Source, counter: &'a mut usize) -> Self {
        Self {
            source,
            counter,
            scopes: Vec::new(),
            hoisted: Vec::new(),
            error: None,
        }
    }
    fn finish(self) -> Result<(), CompilerError> {
        self.error.map_or(Ok(()), Err)
    }
    fn fail(&mut self, error: CompilerError, span: Span) {
        if self.error.is_none() {
            self.error = Some(self.source.locate(error, span));
        }
    }
    /// Hoist `const X = class ...` statements of `stmts`, renaming `X` in
    /// the statements that follow.
    fn statements(&mut self, stmts: &mut Vec<ast::Stmt>) {
        let mut index = 0;
        while index < stmts.len() {
            let hoistable = match &stmts[index] {
                ast::Stmt::Decl(ast::Decl::Var(var)) => bound_class(var).is_some(),
                // A class declaration in a function body (a local class).
                ast::Stmt::Decl(ast::Decl::Class(class)) => !class.declare,
                _ => false,
            };
            if !hoistable || self.scopes.is_empty() {
                index += 1;
                continue;
            }
            let (name, own, class, span, kind) = match stmts.remove(index) {
                ast::Stmt::Decl(ast::Decl::Var(var)) => {
                    let Some((name, class)) = bound_class(&var) else {
                        continue;
                    };
                    let own = class.ident.as_ref().map(|i| i.sym.to_string());
                    (
                        name.clone(),
                        own,
                        class.class.clone(),
                        var.span,
                        "class expression",
                    )
                }
                ast::Stmt::Decl(ast::Decl::Class(declaration)) => {
                    let own = Some(declaration.ident.sym.to_string());
                    let span = declaration.class.span;
                    (
                        declaration.ident,
                        own,
                        declaration.class,
                        span,
                        "local class",
                    )
                }
                _ => continue,
            };
            let shown = own.clone().unwrap_or_else(|| name.sym.to_string());
            *self.counter += 1;
            let target = generated(&[&self.counter.to_string(), &shown]);
            let mut class = class;
            if let Some(captured) = self.captured(&class, &name.sym, own.as_deref()) {
                self.fail(
                    unsupported(&format!(
                        "{kind} that captures the local binding {captured}"
                    )),
                    class.span,
                );
                return;
            }
            // Class expressions nested in this one.
            class.visit_mut_with(self);
            let mut frame = Frame::new();
            frame.insert(name.sym.to_string(), Symbol::plain(target.clone()));
            if let Some(own) = &own {
                frame.insert(own.clone(), Symbol::plain(target.clone()));
            }
            let decl = ast::Decl::Class(ast::ClassDecl {
                ident: ast::Ident::new_no_ctxt(target.as_str().into(), name.span),
                declare: false,
                class,
            });
            match rewrite::declaration(decl, self.source, &[&frame]) {
                Ok(decl) => self
                    .hoisted
                    .push(super::namespaces::item(decl, false, span)),
                Err(error) => {
                    self.error.get_or_insert(error);
                    return;
                }
            }
            for statement in &mut stmts[index..] {
                if let Err(error) = rewrite::statement(statement, self.source, &[&frame]) {
                    self.error.get_or_insert(error);
                    return;
                }
            }
        }
    }
    /// A local binding of an enclosing function that the class references.
    fn captured(&self, class: &ast::Class, binding: &str, own: Option<&str>) -> Option<String> {
        scope::free_references(class)
            .into_iter()
            .filter(|name| name != binding && Some(name.as_str()) != own)
            .find(|name| self.scopes.iter().any(|scope| scope.contains(name)))
    }
    fn scoped(&mut self, names: Names, visit: impl FnOnce(&mut Self)) {
        self.scopes.push(names);
        visit(self);
        self.scopes.pop();
    }
}

impl VisitMut for Hoister<'_> {
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        if let ast::Expr::Class(class) = expr {
            self.fail(
                unsupported("class expression outside a const declaration of its own"),
                class.class.span,
            );
            return;
        }
        expr.visit_mut_children_with(self);
    }
    fn visit_mut_stmts(&mut self, stmts: &mut Vec<ast::Stmt>) {
        self.statements(stmts);
        stmts.visit_mut_children_with(self);
    }
    fn visit_mut_function(&mut self, function: &mut ast::Function) {
        let mut names = Names::new();
        for param in &function.params {
            scope::pattern(&param.pat, &mut names);
        }
        if let Some(body) = &function.body {
            scope::hoisted(&body.stmts, &mut names);
            scope::block(&body.stmts, &mut names);
        }
        self.scoped(names, |this| function.visit_mut_children_with(this));
    }
    fn visit_mut_constructor(&mut self, constructor: &mut ast::Constructor) {
        let mut names = Names::new();
        for param in &constructor.params {
            match param {
                ast::ParamOrTsParamProp::Param(param) => scope::pattern(&param.pat, &mut names),
                ast::ParamOrTsParamProp::TsParamProp(prop) => match &prop.param {
                    ast::TsParamPropParam::Ident(id) => {
                        names.insert(id.id.sym.to_string());
                    }
                    ast::TsParamPropParam::Assign(assign) => {
                        scope::pattern(&assign.left, &mut names)
                    }
                },
            }
        }
        if let Some(body) = &constructor.body {
            scope::hoisted(&body.stmts, &mut names);
            scope::block(&body.stmts, &mut names);
        }
        self.scoped(names, |this| constructor.visit_mut_children_with(this));
    }
    fn visit_mut_arrow_expr(&mut self, arrow: &mut ast::ArrowExpr) {
        let mut names = Names::new();
        for param in &arrow.params {
            scope::pattern(param, &mut names);
        }
        if let ast::ArrowFunctionBody::FunctionBody(body) = &*arrow.body {
            scope::hoisted(&body.stmts, &mut names);
            scope::block(&body.stmts, &mut names);
        }
        self.scoped(names, |this| arrow.visit_mut_children_with(this));
    }
    fn visit_mut_block_stmt(&mut self, block: &mut ast::BlockStmt) {
        let mut names = Names::new();
        scope::block(&block.stmts, &mut names);
        self.scoped(names, |this| block.visit_mut_children_with(this));
    }
    fn visit_mut_for_stmt(&mut self, statement: &mut ast::ForStmt) {
        let mut names = Names::new();
        if let Some(ast::VarDeclOrExpr::VarDecl(var)) = &statement.init {
            for declarator in &var.decls {
                scope::pattern(&declarator.name, &mut names);
            }
        }
        self.scoped(names, |this| statement.visit_mut_children_with(this));
    }
    fn visit_mut_catch_clause(&mut self, clause: &mut ast::CatchClause) {
        let mut names = Names::new();
        if let Some(param) = &clause.param {
            scope::pattern(param, &mut names);
        }
        self.scoped(names, |this| clause.visit_mut_children_with(this));
    }
}
