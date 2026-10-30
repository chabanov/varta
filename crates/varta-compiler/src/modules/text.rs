//! Text resource imports (v38 `text_imports()`):
//! `import css from "./console.css" with { type: "text" }`.
//!
//! The host loader resolves and loads the resource like a module (same path
//! policy, pinned descriptors, graph budgets, read once per identity). Its
//! unit holds the verbatim text as the module constant `default` of type
//! `string`, so it is linked, deduplicated and stored like any other string
//! literal. When the binding is passed directly as the text of
//! `View.stylesheet(id, text)`, the stylesheet grammar is checked at build
//! time with diagnostics inside the resource.
use super::*;
use swc_ecma_visit::{Visit, VisitWith};

/// The import attribute key and the one admitted value.
const TYPE_ATTRIBUTE: &str = "type";
const TEXT_TYPE: &str = "text";
/// The prelude namespace and member that take a stylesheet text.
const VIEW_NAMESPACE: &str = "View";
const STYLESHEET_MEMBER: &str = "stylesheet";
const STYLESHEET_TEXT_ARGUMENT: usize = 1;

/// Check the attributes of an attributed import: exactly `type: "text"`.
pub(super) fn attributes(with: &ast::ObjectLit) -> Result<(), CompilerError> {
    let mut seen = false;
    for property in &with.props {
        let ast::PropOrSpread::Prop(property) = property else {
            return Err(attribute_error("a spread is not an import attribute"));
        };
        let ast::Prop::KeyValue(ast::KeyValueProp { key, value }) = &**property else {
            return Err(attribute_error(
                "import attributes are `key: \"value\"` pairs",
            ));
        };
        let key = match key {
            ast::PropName::Ident(ident) => ident.sym.to_string(),
            ast::PropName::Str(text) => text.value.as_str().unwrap_or_default().to_owned(),
            _ => {
                return Err(attribute_error(
                    "an import attribute key is a name or string",
                ));
            }
        };
        if key != TYPE_ATTRIBUTE {
            return Err(attribute_error(format!(
                "unknown import attribute `{key}`; only `type: \"text\"` is admitted"
            )));
        }
        if seen {
            return Err(attribute_error("duplicate import attribute `type`"));
        }
        seen = true;
        let ast::Expr::Lit(ast::Lit::Str(value)) = &**value else {
            return Err(attribute_error("the `type` import attribute is a string"));
        };
        if value.value.as_str() != Some(TEXT_TYPE) {
            return Err(attribute_error(format!(
                "unsupported import type {:?}; only `type: \"text\"` is admitted",
                value.value.as_str().unwrap_or_default()
            )));
        }
    }
    if seen {
        Ok(())
    } else {
        Err(attribute_error(
            "an attributed import needs `with { type: \"text\" }`",
        ))
    }
}

fn attribute_error(message: impl Into<String>) -> CompilerError {
    CompilerError::Module(message.into())
}

/// The bindings of a text import: one default import, nothing else.
pub(super) fn bindings(import: &ast::ImportDecl) -> Result<Vec<ImportBinding>, CompilerError> {
    if import.type_only {
        return Err(module_error("a text resource has no type-only import"));
    }
    match import.specifiers.as_slice() {
        [ast::ImportSpecifier::Default(default)] => Ok(vec![ImportBinding {
            imported: DEFAULT_EXPORT.into(),
            local: default.local.sym.to_string(),
            span: default.span,
            type_only: false,
        }]),
        _ => Err(module_error(
            "a text resource has only a default import: `import text from \"./x.css\" with { type: \"text\" }`",
        )),
    }
}

/// The unit of a text resource: its verbatim text as `default: string`.
pub(super) fn unit(path: &Path, text: String) -> Result<Unit, CompilerError> {
    let source = Source::new(path, &text);
    let span = Span::new(source.file.start_pos, source.file.end_pos);
    if text.starts_with('\u{feff}') {
        return Err(source.locate(
            module_error(
                "a text resource must not start with a byte order mark (U+FEFF); text is never transformed",
            ),
            span,
        ));
    }
    let ident = ast::Ident::new_no_ctxt(defaults::ANONYMOUS.into(), span);
    let string = ast::TsTypeAnn {
        span,
        type_ann: Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
            span,
            kind: ast::TsKeywordTypeKind::TsStringKeyword,
        })),
    };
    let constant = ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Const,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(ast::BindingIdent {
                id: ident.clone(),
                type_ann: Some(Box::new(string)),
            }),
            init: Some(Box::new(ast::Expr::Lit(ast::Lit::Str(ast::Str {
                span,
                value: text.as_str().into(),
                raw: None,
            })))),
            definite: false,
        }],
    };
    Ok(Unit {
        source,
        ast: ast::Module {
            span,
            body: vec![ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(
                Box::new(constant),
            )))],
            shebang: None,
        },
        imports: Vec::new(),
        exports: vec![Export {
            local: ident.sym.to_string(),
            exported: DEFAULT_EXPORT.into(),
            span,
            type_only: false,
            from: None,
        }],
        stars: Vec::new(),
        text: true,
    })
}

/// Check the stylesheet grammar of every text resource of `importer` passed
/// by its binding as the direct text argument of `View.stylesheet`. A name
/// declared anywhere else in the module may shadow the import, so such a
/// binding is left to the host's patch-time validation.
pub(super) fn check_stylesheets(
    importer: &ast::Module,
    imports: &[Dependency],
    units: &BTreeMap<PathBuf, Unit>,
) -> Result<(), CompilerError> {
    let mut texts = BTreeMap::new();
    for import in imports.iter().filter(|import| import.text) {
        for binding in &import.bindings {
            texts.insert(binding.local.clone(), &import.target);
        }
    }
    if texts.is_empty() {
        return Ok(());
    }
    let mut scan = Scan::default();
    importer.visit_with(&mut scan);
    for name in scan.uses {
        if scan.declared.contains(&name) {
            continue;
        }
        let Some(unit) = texts.get(&name).and_then(|target| units.get(*target)) else {
            continue;
        };
        let text = &unit.source.file.src;
        if let Err(error) = varta_common::view::stylesheet::parse(text) {
            let start = unit.source.file.start_pos + swc_common::BytePos(error.offset as u32);
            let width = text[error.offset..]
                .chars()
                .next()
                .map_or(0, char::len_utf8);
            let span = Span::new(start, start + swc_common::BytePos(width as u32));
            return Err(unit.source.locate(
                CompilerError::Type(format!(
                    "invalid stylesheet for View.stylesheet: {}",
                    error.violation
                )),
                span,
            ));
        }
    }
    Ok(())
}

/// Identifiers passed as the text of `View.stylesheet(id, text)` and every
/// name the module declares (bindings, functions, classes).
#[derive(Default)]
struct Scan {
    uses: BTreeSet<String>,
    declared: BTreeSet<String>,
}
impl Visit for Scan {
    fn visit_call_expr(&mut self, call: &ast::CallExpr) {
        if let ast::Callee::Expr(callee) = &call.callee
            && let ast::Expr::Member(member) = &**callee
            && let ast::Expr::Ident(object) = &*member.obj
            && object.sym == VIEW_NAMESPACE
            && let ast::MemberProp::Ident(property) = &member.prop
            && property.sym == STYLESHEET_MEMBER
            && let Some(argument) = call.args.get(STYLESHEET_TEXT_ARGUMENT)
            && argument.spread.is_none()
            && let ast::Expr::Ident(text) = &*argument.expr
        {
            self.uses.insert(text.sym.to_string());
        }
        call.visit_children_with(self);
    }
    fn visit_binding_ident(&mut self, binding: &ast::BindingIdent) {
        self.declared.insert(binding.id.sym.to_string());
    }
    fn visit_fn_decl(&mut self, function: &ast::FnDecl) {
        self.declared.insert(function.ident.sym.to_string());
        function.visit_children_with(self);
    }
    fn visit_fn_expr(&mut self, function: &ast::FnExpr) {
        if let Some(ident) = &function.ident {
            self.declared.insert(ident.sym.to_string());
        }
        function.visit_children_with(self);
    }
    fn visit_class_decl(&mut self, class: &ast::ClassDecl) {
        self.declared.insert(class.ident.sym.to_string());
        class.visit_children_with(self);
    }
    fn visit_class_expr(&mut self, class: &ast::ClassExpr) {
        if let Some(ident) = &class.ident {
            self.declared.insert(ident.sym.to_string());
        }
        class.visit_children_with(self);
    }
}
