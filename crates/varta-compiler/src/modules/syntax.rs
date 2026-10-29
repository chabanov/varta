use super::*;

fn unsupported(kind: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: kind.into(),
        detail: "outside the local named-function module subset".into(),
    }
}
/// An identifier export/import name; `default` only where `defaults` admits it
/// (v32 `default_exports()`: imported, exported and re-exported names).
fn name(name: &ast::ModuleExportName, defaults: bool) -> Result<String, CompilerError> {
    match name {
        ast::ModuleExportName::Ident(id) if defaults || id.sym != DEFAULT_EXPORT => {
            Ok(id.sym.to_string())
        }
        _ => Err(unsupported("string or default export name")),
    }
}

pub(super) struct Scan {
    pub imports: Vec<Dependency>,
    pub exports: Vec<Export>,
    pub stars: Vec<PathBuf>,
}

pub(super) fn scan(
    module: &mut ast::Module,
    source: &Source,
    path: &Path,
    loader: &dyn ModuleLoader,
    profile: varta_common::language::LanguageProfile,
) -> Result<Scan, CompilerError> {
    let named_types = profile.named_types();
    let reexports = profile.module_reexports();
    let defaults = profile.default_exports();
    let mut imports = Vec::new();
    let mut exports = defaults::lower(module, source, profile)?;
    let mut stars = Vec::new();
    let mut merged = std::collections::BTreeSet::new();
    for item in &module.body {
        let ast::ModuleItem::ModuleDecl(declaration) = item else {
            continue;
        };
        let result = (|| {
            match declaration {
                ast::ModuleDecl::Import(import) => imports.push(import_declaration(
                    import,
                    source,
                    path,
                    loader,
                    (named_types, defaults),
                )?),
                ast::ModuleDecl::ExportDecl(export) => match &export.decl {
                    ast::Decl::Fn(function) => exports.push(Export {
                        local: function.ident.sym.to_string(),
                        exported: function.ident.sym.to_string(),
                        span: function.ident.span,
                        type_only: false,
                        from: None,
                    }),
                    // A later declaration of a merged `export interface` (v32)
                    // adds members to the type already exported.
                    ast::Decl::TsInterface(interface)
                        if profile.interface_extends()
                            && merged.contains(interface.id.sym.as_ref()) => {}
                    ast::Decl::TsInterface(interface) if profile.interface_extends() => {
                        merged.insert(interface.id.sym.to_string());
                        exports.push(Export {
                            local: interface.id.sym.to_string(),
                            exported: interface.id.sym.to_string(),
                            span: interface.id.span,
                            type_only: true,
                            from: None,
                        })
                    }
                    ast::Decl::TsInterface(interface) if named_types => exports.push(Export {
                        local: interface.id.sym.to_string(),
                        exported: interface.id.sym.to_string(),
                        span: interface.id.span,
                        type_only: true,
                        from: None,
                    }),
                    ast::Decl::TsTypeAlias(alias) if named_types => exports.push(Export {
                        local: alias.id.sym.to_string(),
                        exported: alias.id.sym.to_string(),
                        span: alias.id.span,
                        type_only: true,
                        from: None,
                    }),
                    // Classes (v32) live in the type namespace; their value is
                    // resolved from it, so re-exports forward both.
                    ast::Decl::Class(class) if profile.classes() && !class.declare => {
                        exports.push(Export {
                            local: class.ident.sym.to_string(),
                            exported: class.ident.sym.to_string(),
                            span: class.ident.span,
                            type_only: false,
                            from: None,
                        })
                    }
                    // Module constants (v32) join the value export table; the
                    // linker rejects destructuring and non-`const` forms.
                    ast::Decl::Var(var)
                        if profile.module_constants()
                            && !var.declare
                            && var.kind == ast::VarDeclKind::Const =>
                    {
                        for declarator in &var.decls {
                            if let ast::Pat::Ident(binding) = &declarator.name {
                                exports.push(Export {
                                    local: binding.id.sym.to_string(),
                                    exported: binding.id.sym.to_string(),
                                    span: binding.id.span,
                                    type_only: false,
                                    from: None,
                                });
                            }
                        }
                    }
                    _ => {}
                },
                ast::ModuleDecl::ExportNamed(export) => {
                    if export.src.is_some() && !reexports
                        || export.type_only && !named_types
                        || export.with.is_some()
                    {
                        return Err(unsupported("re-export or type-only export"));
                    }
                    let from = match &export.src {
                        Some(src) => {
                            reexport_specifiers(&export.specifiers)?;
                            let target = dependency(src, source, path, loader)?;
                            imports.push(Dependency {
                                target: target.clone(),
                                bindings: Vec::new(),
                                span: src.span,
                            });
                            Some(target)
                        }
                        None => None,
                    };
                    for specifier in &export.specifiers {
                        let ast::ExportSpecifier::Named(specifier) = specifier else {
                            return Err(unsupported("namespace/default export"));
                        };
                        if specifier.is_type_only && !named_types {
                            return Err(unsupported("type-only export"));
                        }
                        // A local list names bindings; only a re-export names `default`.
                        let local = name(&specifier.orig, defaults && from.is_some())?;
                        let exported = specifier
                            .exported
                            .as_ref()
                            .map(|exported| name(exported, defaults))
                            .transpose()?
                            .unwrap_or_else(|| local.clone());
                        exports.push(Export {
                            local,
                            exported,
                            span: specifier.span,
                            type_only: export.type_only || specifier.is_type_only,
                            from: from.clone(),
                        });
                    }
                }
                ast::ModuleDecl::ExportAll(all) if reexports => {
                    if all.type_only || all.with.is_some() {
                        return Err(unsupported("type-only or attributed star re-export"));
                    }
                    let target = dependency(&all.src, source, path, loader)?;
                    imports.push(Dependency {
                        target: target.clone(),
                        bindings: Vec::new(),
                        span: all.src.span,
                    });
                    stars.push(target);
                }
                _ => return Err(unsupported("module export form")),
            }
            Ok(())
        })();
        result.map_err(|e| source.locate(e, declaration.span()))?;
    }
    Ok(Scan {
        imports,
        exports,
        stars,
    })
}

/// `export { … } from` (v32) admits only named specifiers; namespace and
/// default re-export forms keep explicit diagnostics.
fn reexport_specifiers(specifiers: &[ast::ExportSpecifier]) -> Result<(), CompilerError> {
    if specifiers.is_empty() {
        return Err(unsupported("side-effect re-export without names"));
    }
    match specifiers
        .iter()
        .find(|s| !matches!(s, ast::ExportSpecifier::Named(_)))
    {
        Some(ast::ExportSpecifier::Namespace(_)) => {
            Err(unsupported("namespace re-export (export * as name from)"))
        }
        Some(_) => Err(unsupported("default re-export")),
        None => Ok(()),
    }
}

/// Resolve a module specifier through the host loader at its source span.
fn dependency(
    src: &ast::Str,
    source: &Source,
    path: &Path,
    loader: &dyn ModuleLoader,
) -> Result<PathBuf, CompilerError> {
    let specifier = src
        .value
        .as_str()
        .ok_or_else(|| unsupported("non-UTF-8 import path"))?;
    loader
        .resolve(path, specifier)
        .map_err(|e| source.locate(e, src.span))
}

fn import_declaration(
    import: &ast::ImportDecl,
    source: &Source,
    path: &Path,
    loader: &dyn ModuleLoader,
    (named_types, defaults): (bool, bool),
) -> Result<Dependency, CompilerError> {
    if import.type_only && !named_types
        || import.with.is_some()
        || import.phase != ast::ImportPhase::Evaluation
        || import.specifiers.is_empty()
    {
        return Err(unsupported("side-effect, type-only or attributed import"));
    }
    let mut bindings = Vec::new();
    for specifier in &import.specifiers {
        let specifier = match specifier {
            ast::ImportSpecifier::Named(specifier) => specifier,
            // `import x from` (v32) binds the dependency's `default` export.
            ast::ImportSpecifier::Default(default) if defaults => {
                bindings.push(ImportBinding {
                    imported: DEFAULT_EXPORT.into(),
                    local: default.local.sym.to_string(),
                    span: default.span,
                    type_only: import.type_only,
                });
                continue;
            }
            ast::ImportSpecifier::Namespace(_) if defaults => {
                return Err(unsupported("namespace import (import * as name)"));
            }
            _ => return Err(unsupported("default or namespace import")),
        };
        if specifier.is_type_only && !named_types {
            return Err(unsupported("type-only import"));
        }
        let imported = specifier
            .imported
            .as_ref()
            .map(|imported| name(imported, defaults))
            .transpose()?
            .unwrap_or_else(|| specifier.local.sym.to_string());
        bindings.push(ImportBinding {
            imported,
            local: specifier.local.sym.to_string(),
            span: specifier.span,
            type_only: import.type_only || specifier.is_type_only,
        });
    }
    Ok(Dependency {
        target: dependency(&import.src, source, path, loader)?,
        bindings,
        span: import.src.span,
    })
}
