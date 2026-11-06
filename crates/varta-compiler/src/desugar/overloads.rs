//! Module-level function overload signatures. TypeScript requires them to
//! precede their implementation directly, all with the same export state.
//! Each signature becomes an unexported `declare function` (the only body-
//! less function form lowering admits), which `lower::overloads` attaches
//! to the implementation; a program's own ambient functions are rejected
//! here first.
use super::{type_error, unsupported};
use crate::CompilerError;
use crate::source::Source;
use swc_ecma_ast as ast;

pub(super) fn mark(module: &mut ast::Module, source: &Source) -> Result<(), CompilerError> {
    let mut pending: Option<(String, bool, swc_common::Span)> = None;
    for item in &mut module.body {
        let (function, exported) = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Fn(function))) => (function, false),
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(ast::ExportDecl {
                decl: ast::Decl::Fn(function),
                ..
            })) => (function, true),
            _ => {
                if let Some((name, _, span)) = pending.take() {
                    return Err(missing(&name, source, span));
                }
                continue;
            }
        };
        let name = function.ident.sym.to_string();
        if function.declare {
            return Err(source.locate(
                unsupported("ambient function declaration"),
                function.ident.span,
            ));
        }
        if let Some((pending_name, pending_export, span)) = &pending {
            if *pending_name != name {
                return Err(missing(pending_name, source, *span));
            }
            if *pending_export != exported {
                return Err(source.locate(
                    type_error("overload signatures must all be exported or non-exported (TS2383)"),
                    function.ident.span,
                ));
            }
        }
        if function.function.body.is_some() {
            pending = None;
            continue;
        }
        pending = Some((name, exported, function.ident.span));
        let mut signature = function.clone();
        signature.declare = true;
        *item = ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Fn(signature)));
    }
    match pending {
        Some((name, _, span)) => Err(missing(&name, source, span)),
        None => Ok(()),
    }
}

fn missing(name: &str, source: &Source, span: swc_common::Span) -> CompilerError {
    source.locate(
        type_error(format!(
            "function implementation of {} is missing or not immediately following the declaration (TS2391)",
            super::display_name(name)
        )),
        span,
    )
}
