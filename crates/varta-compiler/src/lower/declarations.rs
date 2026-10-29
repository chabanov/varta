use super::*;
pub(super) fn collect<'a>(
    module: &'a ast::Module,
    source: &Source,
    profile: varta_common::language::LanguageProfile,
) -> Result<Vec<&'a ast::FnDecl>> {
    let mut declarations = Vec::new();
    for item in &module.body {
        if super::tools::is_declaration(item) || super::http::is_declaration(item) {
            continue;
        }
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => decl,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
            ast::ModuleItem::Stmt(ast::Stmt::Empty(_)) => continue,
            ast::ModuleItem::ModuleDecl(
                ast::ModuleDecl::Import(_) | ast::ModuleDecl::ExportNamed(_),
            ) => continue,
            _ => {
                return Err(source.locate(
                    unsupported("top-level statement or module import"),
                    item.span(),
                ));
            }
        };
        match declaration {
            ast::Decl::Fn(function) if !function.declare => declarations.push(function),
            ast::Decl::TsInterface(interface)
                if matches!(
                    interface.id.sym.as_ref(),
                    "VartaResult"
                        | "Array"
                        | "VartaHttpRequest"
                        | "VartaHttpResponse"
                        | "VartaHttpHeader"
                ) =>
            {
                return Err(source.locate(
                    unsupported(&format!(
                        "redeclaring the built-in {} type",
                        interface.id.sym
                    )),
                    interface.id.span,
                ));
            }
            ast::Decl::TsTypeAlias(alias)
                if matches!(
                    alias.id.sym.as_ref(),
                    "VartaResult"
                        | "Array"
                        | "VartaHttpRequest"
                        | "VartaHttpResponse"
                        | "VartaHttpHeader"
                ) =>
            {
                return Err(source.locate(
                    unsupported(&format!("redeclaring the built-in {} type", alias.id.sym)),
                    alias.id.span,
                ));
            }
            ast::Decl::TsInterface(_) | ast::Decl::TsTypeAlias(_) => {}
            // Classes are laid out and lowered by `classes` (v32).
            ast::Decl::Class(class) if profile.classes() && !class.declare => {}
            ast::Decl::Var(var) if var.declare => {
                for declaration in &var.decls {
                    let ast::Pat::Ident(binding) = &declaration.name else {
                        return Err(
                            source.locate(unsupported("ambient declaration"), declaration.span())
                        );
                    };
                    if !matches!(
                        binding.id.sym.as_ref(),
                        "varta" | "console" | "log" | "JSON"
                    ) || declaration.init.is_some()
                    {
                        return Err(
                            source.locate(unsupported("ambient declaration"), declaration.span())
                        );
                    }
                }
            }
            _ => {
                return Err(source.locate(unsupported("top-level declaration"), declaration.span()));
            }
        }
    }
    Ok(declarations)
}
