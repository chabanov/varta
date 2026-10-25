use super::*;
pub(super) fn collect<'a>(
    module: &'a ast::Module,
    source: &Source,
) -> Result<Vec<&'a ast::FnDecl>> {
    let mut declarations = Vec::new();
    for item in &module.body {
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
            ast::Decl::TsInterface(interface) if interface.id.sym == "VartaResult" => {
                return Err(source.locate(
                    unsupported("redeclaring the built-in VartaResult type"),
                    interface.id.span,
                ));
            }
            ast::Decl::TsTypeAlias(alias) if alias.id.sym == "VartaResult" => {
                return Err(source.locate(
                    unsupported("redeclaring the built-in VartaResult type"),
                    alias.id.span,
                ));
            }
            ast::Decl::TsInterface(_) | ast::Decl::TsTypeAlias(_) => {}
            ast::Decl::Var(var) if var.declare => {
                for declaration in &var.decls {
                    let ast::Pat::Ident(binding) = &declaration.name else {
                        return Err(
                            source.locate(unsupported("ambient declaration"), declaration.span())
                        );
                    };
                    if !matches!(binding.id.sym.as_ref(), "varta" | "console" | "log")
                        || declaration.init.is_some()
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
