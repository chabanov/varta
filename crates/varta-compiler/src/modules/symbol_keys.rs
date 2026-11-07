//! Symbol-keyed members (v38 `symbol_values`, K9 D2; ECMA-262 §6.1.7 and
//! TypeScript's `unique symbol` property keys).
//!
//! A `unique symbol` constant names one symbol for the whole program, so a
//! computed key `[k]` of such a constant names a statically known member,
//! as TypeScript types it. Before lowering, every computed key of an
//! object literal, member access, class member or type member whose key is
//! a module-level `unique symbol` constant (declared here, imported, or
//! re-exported) or a well-known symbol other than the iterator symbols is
//! rewritten to an ordinary member named by its stable key ([`key`]):
//! the private-name prefix, so no serializer, `Object.keys`, `for...in` or
//! KV reader treats it as a string-keyed property, then `[`, the declaring
//! module's graph index and the constant's name. The key never depends on
//! the symbol value, which is created once per instance.
use super::{Dependency, Export, Unit};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitMut, VisitMutWith, VisitWith};

/// The exported `unique symbol` constants of a module, by exported name,
/// with their member keys.
pub(crate) type Keys = BTreeMap<String, String>;

const SYMBOL_GLOBAL: &str = crate::modules::engine::SYMBOL_GLOBAL;
/// The well-known symbols whose computed keys name ordinary members here;
/// `Symbol.iterator` and `Symbol.asyncIterator` keep their method names
/// (`lower::classes::ITERATOR_METHOD`).
pub(crate) const WELL_KNOWN_KEYS: [&str; 3] = ["toPrimitive", "toStringTag", "hasInstance"];

/// The member key of the `unique symbol` constant `name` declared by the
/// module at graph index `module`.
#[must_use]
pub(crate) fn key(module: usize, name: &str) -> String {
    format!(
        "{}[{module}:{name}]",
        varta_common::value::PRIVATE_KEY_PREFIX
    )
}

/// Whether `name` is a symbol member key ([`key`], [`well_known_key`]).
#[must_use]
pub(crate) fn is_symbol_key(name: &str) -> bool {
    name.strip_prefix(varta_common::value::PRIVATE_KEY_PREFIX)
        .is_some_and(|rest| rest.starts_with('['))
}

/// The member key of the well-known symbol `Symbol.<name>`.
#[must_use]
pub(crate) fn well_known_key(name: &str) -> String {
    format!(
        "{}[{SYMBOL_GLOBAL}.{name}]",
        varta_common::value::PRIVATE_KEY_PREFIX
    )
}

/// Whether the initializer or annotation of `declarator` makes it a
/// `unique symbol` constant: `Symbol(...)` or a `unique symbol` annotation.
fn unique(declarator: &ast::VarDeclarator, shadowed: bool) -> bool {
    let ast::Pat::Ident(binding) = &declarator.name else {
        return false;
    };
    let annotated = binding.type_ann.as_ref().is_some_and(|annotation| {
        matches!(&*annotation.type_ann, ast::TsType::TsTypeOperator(operator)
            if operator.op == ast::TsTypeOperatorOp::Unique)
    });
    let created = declarator.init.as_deref().is_some_and(|init| {
        matches!(init, ast::Expr::Call(ast::CallExpr { callee: ast::Callee::Expr(callee), .. })
            if !shadowed && matches!(&**callee, ast::Expr::Ident(id) if id.sym == SYMBOL_GLOBAL))
    });
    annotated || binding.type_ann.is_none() && created
}

/// Names bound anywhere in `module` with how often each is bound.
fn bindings(module: &ast::Module) -> BTreeMap<String, usize> {
    #[derive(Default)]
    struct Counts(BTreeMap<String, usize>);
    impl Visit for Counts {
        fn visit_binding_ident(&mut self, binding: &ast::BindingIdent) {
            *self.0.entry(binding.id.sym.to_string()).or_default() += 1;
        }
        fn visit_fn_decl(&mut self, function: &ast::FnDecl) {
            *self.0.entry(function.ident.sym.to_string()).or_default() += 1;
            function.visit_children_with(self);
        }
        fn visit_class_decl(&mut self, class: &ast::ClassDecl) {
            *self.0.entry(class.ident.sym.to_string()).or_default() += 1;
            class.visit_children_with(self);
        }
    }
    let mut counts = Counts::default();
    module.visit_with(&mut counts);
    counts.0
}

/// Rewrite the symbol-keyed members of the module at graph index `index`
/// and return its exported keys. `units` holds every dependency.
pub(crate) fn rewrite(
    module: &mut ast::Module,
    index: usize,
    (imports, exports): (&[Dependency], &[Export]),
    units: &BTreeMap<PathBuf, Unit>,
) -> Keys {
    let counts = bindings(module);
    let bound_once = |name: &str| counts.get(name).copied().unwrap_or_default() <= 1;
    let symbol_shadowed = counts.contains_key(SYMBOL_GLOBAL);
    // Local name -> member key.
    let mut local: BTreeMap<String, String> = BTreeMap::new();
    for item in &module.body {
        let var = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(ast::Decl::Var(var))) => var,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(ast::ExportDecl {
                decl: ast::Decl::Var(var),
                ..
            })) => var,
            _ => continue,
        };
        if var.kind != ast::VarDeclKind::Const || var.declare {
            continue;
        }
        for declarator in &var.decls {
            if let ast::Pat::Ident(binding) = &declarator.name
                && unique(declarator, symbol_shadowed)
                && bound_once(binding.id.sym.as_ref())
            {
                let name = binding.id.sym.to_string();
                local.insert(name.clone(), key(index, &name));
            }
        }
    }
    let declared: BTreeSet<String> = local.keys().cloned().collect();
    for import in imports {
        let Some(target) = units.get(&import.target) else {
            continue;
        };
        for binding in &import.bindings {
            if let Some(key) = target.symbols.get(&binding.imported)
                && !counts.contains_key(&binding.local)
            {
                local.insert(binding.local.clone(), key.clone());
            }
        }
    }
    let mut exported = Keys::new();
    for export in exports.iter().filter(|export| !export.type_only) {
        let key = match &export.from {
            Some(path) => units
                .get(path)
                .and_then(|unit| unit.symbols.get(&export.local))
                .cloned(),
            None => local.get(&export.local).cloned(),
        };
        if let Some(key) = key {
            exported.insert(export.exported.clone(), key);
        }
    }
    for name in &declared {
        // `export const k = Symbol()` is exported under its own name.
        if exports
            .iter()
            .any(|export| export.from.is_none() && export.local == *name)
        {
            exported
                .entry(name.clone())
                .or_insert_with(|| local[name].clone());
        }
    }
    module.visit_mut_with(&mut Rewriter {
        keys: &local,
        well_known: !symbol_shadowed,
    });
    exported
}

struct Rewriter<'a> {
    keys: &'a BTreeMap<String, String>,
    well_known: bool,
}
impl Rewriter<'_> {
    /// The member key a computed key expression names, if it is symbolic.
    fn key(&self, expression: &ast::Expr) -> Option<String> {
        match expression {
            ast::Expr::Paren(paren) => self.key(&paren.expr),
            ast::Expr::Ident(id) => self.keys.get(id.sym.as_ref()).cloned(),
            ast::Expr::Member(ast::MemberExpr {
                obj,
                prop: ast::MemberProp::Ident(name),
                ..
            }) if self.well_known
                && matches!(&**obj, ast::Expr::Ident(id) if id.sym == SYMBOL_GLOBAL)
                && WELL_KNOWN_KEYS.contains(&name.sym.as_ref()) =>
            {
                Some(well_known_key(name.sym.as_ref()))
            }
            _ => None,
        }
    }
    fn name(key: String, span: swc_common::Span) -> ast::IdentName {
        ast::IdentName::new(key.into(), span)
    }
}
impl VisitMut for Rewriter<'_> {
    fn visit_mut_prop_name(&mut self, name: &mut ast::PropName) {
        name.visit_mut_children_with(self);
        if let ast::PropName::Computed(computed) = name
            && let Some(key) = self.key(&computed.expr)
        {
            *name = ast::PropName::Ident(Self::name(key, computed.span));
        }
    }
    fn visit_mut_member_prop(&mut self, prop: &mut ast::MemberProp) {
        prop.visit_mut_children_with(self);
        if let ast::MemberProp::Computed(computed) = prop
            && let Some(key) = self.key(&computed.expr)
        {
            *prop = ast::MemberProp::Ident(Self::name(key, computed.span));
        }
    }
    fn visit_mut_super_prop(&mut self, prop: &mut ast::SuperProp) {
        prop.visit_mut_children_with(self);
        if let ast::SuperProp::Computed(computed) = prop
            && let Some(key) = self.key(&computed.expr)
        {
            *prop = ast::SuperProp::Ident(Self::name(key, computed.span));
        }
    }
    fn visit_mut_ts_property_signature(&mut self, signature: &mut ast::TsPropertySignature) {
        signature.visit_mut_children_with(self);
        if signature.computed
            && let Some(key) = self.key(&signature.key)
        {
            *signature.key = ast::Expr::Ident(ast::Ident::new_no_ctxt(key.into(), signature.span));
            signature.computed = false;
        }
    }
    fn visit_mut_ts_method_signature(&mut self, signature: &mut ast::TsMethodSignature) {
        signature.visit_mut_children_with(self);
        if signature.computed
            && let Some(key) = self.key(&signature.key)
        {
            *signature.key = ast::Expr::Ident(ast::Ident::new_no_ctxt(key.into(), signature.span));
            signature.computed = false;
        }
    }
    fn visit_mut_ts_getter_signature(&mut self, signature: &mut ast::TsGetterSignature) {
        signature.visit_mut_children_with(self);
        if signature.computed
            && let Some(key) = self.key(&signature.key)
        {
            *signature.key = ast::Expr::Ident(ast::Ident::new_no_ctxt(key.into(), signature.span));
            signature.computed = false;
        }
    }
}
