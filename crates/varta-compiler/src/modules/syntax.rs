use super::*;

fn unsupported(kind: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: kind.into(),
        detail: "outside the local named-function module subset".into(),
    }
}
fn name(name: &ast::ModuleExportName) -> Result<String, CompilerError> {
    match name {
        ast::ModuleExportName::Ident(id) if id.sym != "default" => Ok(id.sym.to_string()),
        _ => Err(unsupported("string or default export name")),
    }
}

pub(super) fn scan(
    module: &ast::Module,
    source: &Source,
    path: &Path,
    loader: &dyn ModuleLoader,
) -> Result<(Vec<Dependency>, Vec<Export>), CompilerError> {
    let mut imports = Vec::new();
    let mut exports = Vec::new();
    for item in &module.body {
        let ast::ModuleItem::ModuleDecl(declaration) = item else {
            continue;
        };
        let result = (|| {
            match declaration {
                ast::ModuleDecl::Import(import) => {
                    imports.push(import_declaration(import, source, path, loader)?)
                }
                ast::ModuleDecl::ExportDecl(export) => {
                    if let ast::Decl::Fn(function) = &export.decl {
                        exports.push(Export {
                            local: function.ident.sym.to_string(),
                            exported: function.ident.sym.to_string(),
                            span: function.ident.span,
                        });
                    }
                }
                ast::ModuleDecl::ExportNamed(export) => {
                    if export.src.is_some() || export.type_only || export.with.is_some() {
                        return Err(unsupported("re-export or type-only export"));
                    }
                    for specifier in &export.specifiers {
                        let ast::ExportSpecifier::Named(specifier) = specifier else {
                            return Err(unsupported("namespace/default export"));
                        };
                        if specifier.is_type_only {
                            return Err(unsupported("type-only export"));
                        }
                        let local = name(&specifier.orig)?;
                        let exported = specifier
                            .exported
                            .as_ref()
                            .map(name)
                            .transpose()?
                            .unwrap_or_else(|| local.clone());
                        exports.push(Export {
                            local,
                            exported,
                            span: specifier.span,
                        });
                    }
                }
                _ => return Err(unsupported("module export form")),
            }
            Ok(())
        })();
        result.map_err(|e| source.locate(e, declaration.span()))?;
    }
    Ok((imports, exports))
}

fn import_declaration(
    import: &ast::ImportDecl,
    source: &Source,
    path: &Path,
    loader: &dyn ModuleLoader,
) -> Result<Dependency, CompilerError> {
    if import.type_only
        || import.with.is_some()
        || import.phase != ast::ImportPhase::Evaluation
        || import.specifiers.is_empty()
    {
        return Err(unsupported("side-effect, type-only or attributed import"));
    }
    let mut bindings = Vec::new();
    for specifier in &import.specifiers {
        let ast::ImportSpecifier::Named(specifier) = specifier else {
            return Err(unsupported("default or namespace import"));
        };
        if specifier.is_type_only {
            return Err(unsupported("type-only import"));
        }
        let imported = specifier
            .imported
            .as_ref()
            .map(name)
            .transpose()?
            .unwrap_or_else(|| specifier.local.sym.to_string());
        bindings.push(ImportBinding {
            imported,
            local: specifier.local.sym.to_string(),
            span: specifier.span,
        });
    }
    let specifier = import
        .src
        .value
        .as_str()
        .ok_or_else(|| unsupported("non-UTF-8 import path"))?;
    Ok(Dependency {
        target: loader
            .resolve(path, specifier)
            .map_err(|e| source.locate(e, import.src.span))?,
        bindings,
        span: import.src.span,
    })
}
