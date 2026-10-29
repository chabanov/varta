//! Default exports (v32 `default_exports()`). A default export is an ordinary
//! binding exported under the name `default`: the scanner rewrites each
//! `export default` item into the plain top-level declaration it introduces
//! and records an [`Export`], so functions, classes, interfaces and module
//! constants are linked by their existing rules and the export tables treat
//! `default` like any other name (except that `export *` never forwards it).
use super::*;

/// ECMAScript's local binding name of an anonymous default export. No source
/// identifier can spell it, so it never collides with or is read by user code.
pub(super) const ANONYMOUS: &str = "*default*";

/// Rewrite the default exports of `module` in place and return their exports.
/// `export default <identifier>` exports that binding in every namespace;
/// any other expression becomes the module constant `*default*`.
pub(super) fn lower(
    module: &mut ast::Module,
    source: &Source,
    profile: varta_common::language::LanguageProfile,
) -> Result<Vec<Export>, CompilerError> {
    let mut exports = Vec::new();
    if !profile.default_exports() {
        return Ok(exports);
    }
    for index in 0..module.body.len() {
        let ast::ModuleItem::ModuleDecl(declaration) = &module.body[index] else {
            continue;
        };
        let span = declaration.span();
        let (item, export) = match declaration {
            ast::ModuleDecl::ExportDefaultDecl(default) => declaration_item(&default.decl, span),
            ast::ModuleDecl::ExportDefaultExpr(default) => {
                expression_item(&default.expr, index, module, profile)
                    .map_err(|e| source.locate(e, span))?
            }
            _ => continue,
        };
        // Report a second default before its binding can collide.
        if !exports.is_empty() {
            return Err(source.locate(
                CompilerError::Module(format!("duplicate export: {DEFAULT_EXPORT}")),
                span,
            ));
        }
        module.body[index] = item;
        exports.push(export);
    }
    Ok(exports)
}

fn export(local: &ast::Ident, type_only: bool) -> Export {
    Export {
        local: local.sym.to_string(),
        exported: DEFAULT_EXPORT.into(),
        span: local.span,
        type_only,
        from: None,
    }
}

fn binding(ident: Option<&ast::Ident>, span: Span) -> ast::Ident {
    ident
        .cloned()
        .unwrap_or_else(|| ast::Ident::new_no_ctxt(ANONYMOUS.into(), span))
}

fn declaration_item(default: &ast::DefaultDecl, span: Span) -> (ast::ModuleItem, Export) {
    let (declaration, export) = match default {
        ast::DefaultDecl::Fn(function) => {
            let ident = binding(function.ident.as_ref(), span);
            let export = export(&ident, false);
            let declaration = ast::Decl::Fn(ast::FnDecl {
                ident,
                declare: false,
                function: function.function.clone(),
            });
            (declaration, export)
        }
        ast::DefaultDecl::Class(class) => {
            let ident = binding(class.ident.as_ref(), span);
            let export = export(&ident, false);
            let declaration = ast::Decl::Class(ast::ClassDecl {
                ident,
                declare: false,
                class: class.class.clone(),
            });
            (declaration, export)
        }
        ast::DefaultDecl::TsInterfaceDecl(interface) => (
            ast::Decl::TsInterface(interface.clone()),
            export(&interface.id, true),
        ),
    };
    (ast::ModuleItem::Stmt(ast::Stmt::Decl(declaration)), export)
}

fn expression_item(
    expr: &ast::Expr,
    index: usize,
    module: &ast::Module,
    profile: varta_common::language::LanguageProfile,
) -> Result<(ast::ModuleItem, Export), CompilerError> {
    let span = expr.span();
    if let ast::Expr::Ident(ident) = expr
        && bound(module, &ident.sym)
    {
        if declared_after(&module.body[index + 1..], &ident.sym) {
            return Err(CompilerError::Invalid(format!(
                "default export of {} precedes its declaration",
                ident.sym
            )));
        }
        let empty = ast::ModuleItem::Stmt(ast::Stmt::Empty(ast::EmptyStmt { span }));
        return Ok((empty, export(ident, false)));
    }
    if !profile.module_constants() {
        return Err(CompilerError::Unsupported {
            kind: "default export expression".into(),
            detail: "outside the local named-function module subset".into(),
        });
    }
    let ident = ast::Ident::new_no_ctxt(ANONYMOUS.into(), span);
    let export = export(&ident, false);
    let constant = ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(ast::BindingIdent {
                id: ident,
                type_ann: None,
            }),
            init: Some(Box::new(expr.clone())),
            definite: false,
        }],
    };
    let item = ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(Box::new(constant))));
    Ok((item, export))
}

/// Whether `name` is a module binding (declaration or import). Other
/// identifiers, such as `undefined`, are default-exported as values.
fn bound(module: &ast::Module, name: &str) -> bool {
    module.body.iter().any(|item| match item {
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::Import(import)) => {
            import.specifiers.iter().any(|specifier| {
                let local = match specifier {
                    ast::ImportSpecifier::Named(named) => &named.local,
                    ast::ImportSpecifier::Default(default) => &default.local,
                    ast::ImportSpecifier::Namespace(namespace) => &namespace.local,
                };
                local.sym == name
            })
        }
        ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => {
            declares(&export.decl, name)
        }
        ast::ModuleItem::Stmt(ast::Stmt::Decl(declaration)) => declares(declaration, name),
        _ => false,
    })
}
fn declares(declaration: &ast::Decl, name: &str) -> bool {
    match declaration {
        ast::Decl::Fn(function) => function.ident.sym == name,
        ast::Decl::TsInterface(interface) => interface.id.sym == name,
        ast::Decl::TsTypeAlias(alias) => alias.id.sym == name,
        _ => lexical(declaration, name),
    }
}
/// Whether a class or variable declaration binds `name` (lexically scoped
/// bindings with a temporal dead zone).
fn lexical(declaration: &ast::Decl, name: &str) -> bool {
    match declaration {
        ast::Decl::Class(class) => class.ident.sym == name,
        ast::Decl::Var(var) => var
            .decls
            .iter()
            .any(|declarator| matches!(&declarator.name, ast::Pat::Ident(id) if id.id.sym == name)),
        _ => false,
    }
}

/// Whether a later top-level `const`/`let`/`var` or class declares `name`:
/// exporting it by value before that point would read it in its temporal dead
/// zone. Function declarations are hoisted and imports are already bound.
fn declared_after(items: &[ast::ModuleItem], name: &str) -> bool {
    items.iter().any(|item| {
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(declaration)) => declaration,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
            _ => return false,
        };
        lexical(declaration, name)
    })
}
