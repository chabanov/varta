//! Static class fields and `this` in static methods of module-level
//! classes.
//!
//! A static field `static x: T = v` of class `C` becomes the module
//! variable `let __vartaDecl$C$x: T = v` declared right after the class, so
//! its initializer runs in the module initializer sequence at the class's
//! position, as ECMAScript evaluates static fields at class definition.
//! `C.x` (and `this.x` in static code) becomes that variable, merged with
//! the class like a namespace member. A subclass reaches an inherited field
//! through the same variable for reads; writing it through the subclass
//! would create an own property of the subclass in JavaScript and is
//! rejected. `private`/`protected` and `readonly` keep TypeScript's checks.
//!
//! `this` in a static method or initializer is the class itself; a static
//! method that uses `this` is copied into each module-level subclass that
//! inherits it, with `this` naming that subclass, since static calls are
//! resolved against the named class. Static blocks and initializers that
//! are not in the module initializer subset need an evaluation-order design
//! for effects at class definition and are rejected.
use super::{generated, type_error, unsupported};
use crate::CompilerError;
use crate::source::Source;
use std::collections::{BTreeMap, BTreeSet};
use swc_common::{Span, Spanned};
use swc_ecma_ast as ast;
use swc_ecma_visit::{Visit, VisitMut, VisitMutWith, VisitWith};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Access {
    Public,
    Protected,
    Private,
}

/// A static field reached through a class.
#[derive(Clone, Debug)]
pub(crate) struct Field {
    /// The declaring class.
    pub owner: String,
    pub access: Access,
    pub readonly: bool,
    /// Reached through a subclass of the declaring class.
    pub inherited: bool,
}

/// The static members of one module-level class.
#[derive(Clone, Default)]
pub(super) struct Statics {
    pub parent: Option<String>,
    /// Own and inherited fields: name, field, generated variable.
    pub fields: Vec<(String, Field, String)>,
    /// Static methods that use `this`, own and inherited, as declared.
    this_methods: Vec<ast::ClassMethod>,
}

/// Module-level classes by name, with their parent class names.
pub(super) type Classes = BTreeMap<String, Statics>;

fn key(key: &ast::PropName) -> Option<String> {
    match key {
        ast::PropName::Ident(id) => Some(id.sym.to_string()),
        ast::PropName::Str(text) => text.value.as_str().map(str::to_owned),
        _ => None,
    }
}

fn access(accessibility: Option<ast::Accessibility>) -> Access {
    match accessibility {
        Some(ast::Accessibility::Private) => Access::Private,
        Some(ast::Accessibility::Protected) => Access::Protected,
        _ => Access::Public,
    }
}

/// Whether a function's own code uses `this` (outside nested non-arrow
/// functions).
fn uses_this(function: &ast::Function) -> bool {
    let mut finder = ThisFinder(false);
    function.params.visit_with(&mut finder);
    function.body.visit_with(&mut finder);
    finder.0
}
pub(super) struct ThisFinder(bool);
impl Visit for ThisFinder {
    fn visit_this_expr(&mut self, _: &ast::ThisExpr) {
        self.0 = true;
    }
    fn visit_function(&mut self, _: &ast::Function) {}
    fn visit_class(&mut self, _: &ast::Class) {}
    fn visit_getter_prop(&mut self, _: &ast::GetterProp) {}
    fn visit_setter_prop(&mut self, _: &ast::SetterProp) {}
}

/// The module-level class declarations of `items`.
fn classes(items: &[ast::ModuleItem]) -> Vec<&ast::ClassDecl> {
    items
        .iter()
        .filter_map(|item| match super::declaration(item) {
            Some((ast::Decl::Class(class), _)) => Some(class),
            _ => None,
        })
        .collect()
}

/// Collect the static fields and `this`-using static methods of every
/// module-level class, resolving inheritance between them.
pub(super) fn collect(
    items: &[ast::ModuleItem],
    source: &Source,
) -> Result<Classes, CompilerError> {
    let declarations: BTreeMap<String, &ast::ClassDecl> = classes(items)
        .into_iter()
        .map(|c| (c.ident.sym.to_string(), c))
        .collect();
    let mut resolved = Classes::new();
    for name in declarations.keys() {
        resolve(name, &declarations, &mut resolved, source, 0)?;
    }
    Ok(resolved)
}

fn resolve(
    name: &str,
    declarations: &BTreeMap<String, &ast::ClassDecl>,
    resolved: &mut Classes,
    source: &Source,
    depth: usize,
) -> Result<(), CompilerError> {
    if resolved.contains_key(name) {
        return Ok(());
    }
    let declaration = declarations[name];
    let class = &declaration.class;
    let parent = match class.super_class.as_deref() {
        Some(ast::Expr::Ident(id)) if declarations.contains_key(id.sym.as_ref()) => {
            Some(id.sym.to_string())
        }
        _ => None,
    };
    let mut statics = Statics {
        parent: parent.clone(),
        ..Statics::default()
    };
    if let Some(parent) = &parent
        && depth < declarations.len()
    {
        resolve(parent, declarations, resolved, source, depth + 1)?;
        let inherited = resolved[parent].clone();
        statics.fields = inherited
            .fields
            .into_iter()
            .map(|(name, mut field, variable)| {
                field.inherited = true;
                (name, field, variable)
            })
            .collect();
        statics.this_methods = inherited
            .this_methods
            .into_iter()
            .filter(|method| {
                let name = key(&method.key);
                !class.body.iter().any(|member| {
                    matches!(member, ast::ClassMember::Method(own)
                        if own.is_static && key(&own.key) == name)
                })
            })
            .collect();
    }
    for member in &class.body {
        let member = static_accessor(member);
        match member.as_ref() {
            ast::ClassMember::ClassProp(property) if property.is_static => {
                let Some(field) = key(&property.key) else {
                    return Err(source.locate(
                        unsupported("computed static class field name"),
                        property.key.span(),
                    ));
                };
                statics.fields.retain(|(name, ..)| *name != field);
                statics.fields.push((
                    field.clone(),
                    Field {
                        owner: name.to_owned(),
                        access: access(property.accessibility),
                        readonly: property.readonly,
                        inherited: false,
                    },
                    generated(&[name, &field]),
                ));
            }
            ast::ClassMember::Method(method) if method.is_static && uses_this(&method.function) => {
                let name = key(&method.key);
                statics.this_methods.retain(|m| key(&m.key) != name);
                statics.this_methods.push(method.clone());
            }
            _ => {}
        }
    }
    resolved.insert(name.to_owned(), statics);
    Ok(())
}

/// A static auto-accessor `static accessor x = v` is a static field: the
/// class object exposes no property descriptors, so the get/set pair over
/// private storage cannot be told apart from the field.
fn static_accessor(member: &ast::ClassMember) -> std::borrow::Cow<'_, ast::ClassMember> {
    match member {
        ast::ClassMember::AutoAccessor(accessor)
            if accessor.is_static
                && accessor.decorators.is_empty()
                && matches!(accessor.key, ast::Key::Public(_)) =>
        {
            let ast::Key::Public(key) = &accessor.key else {
                return std::borrow::Cow::Borrowed(member);
            };
            std::borrow::Cow::Owned(ast::ClassMember::ClassProp(ast::ClassProp {
                span: accessor.span,
                key: key.clone(),
                value: accessor.value.clone(),
                type_ann: accessor.type_ann.clone(),
                is_static: true,
                decorators: Vec::new(),
                accessibility: accessor.accessibility,
                is_abstract: accessor.is_abstract,
                is_optional: false,
                is_override: accessor.is_override,
                readonly: false,
                declare: false,
                definite: accessor.definite,
            }))
        }
        member => std::borrow::Cow::Borrowed(member),
    }
}

/// Replace `this` (outside nested non-arrow functions) by the class name.
struct ThisToClass<'a> {
    name: &'a str,
    error: Option<Span>,
}
impl VisitMut for ThisToClass<'_> {
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        match expr {
            ast::Expr::This(this) => {
                *expr = ast::Expr::Ident(ast::Ident::new_no_ctxt(self.name.into(), this.span));
            }
            ast::Expr::SuperProp(property) => {
                self.error.get_or_insert(property.span);
            }
            _ => expr.visit_mut_children_with(self),
        }
    }
    fn visit_mut_function(&mut self, _: &mut ast::Function) {}
    fn visit_mut_class(&mut self, _: &mut ast::Class) {}
    fn visit_mut_getter_prop(&mut self, _: &mut ast::GetterProp) {}
    fn visit_mut_setter_prop(&mut self, _: &mut ast::SetterProp) {}
}

fn this_to_class(
    function: &mut ast::Function,
    name: &str,
    source: &Source,
) -> Result<(), CompilerError> {
    let mut rewrite = ThisToClass { name, error: None };
    function.params.visit_mut_with(&mut rewrite);
    if let Some(body) = &mut function.body {
        body.visit_mut_with(&mut rewrite);
    }
    match rewrite.error {
        Some(span) => Err(source.locate(unsupported("super in static class code"), span)),
        None => Ok(()),
    }
}

/// `C.m` member references of a module, by class and member name.
pub(super) fn references(items: &[ast::ModuleItem]) -> BTreeSet<(String, String)> {
    struct References(BTreeSet<(String, String)>);
    impl Visit for References {
        fn visit_member_expr(&mut self, member: &ast::MemberExpr) {
            if let (ast::Expr::Ident(class), ast::MemberProp::Ident(name)) =
                (&*member.obj, &member.prop)
            {
                self.0.insert((class.sym.to_string(), name.sym.to_string()));
            }
            member.visit_children_with(self);
        }
    }
    let mut references = References(BTreeSet::new());
    items.visit_with(&mut references);
    references.0
}

/// Lower the static members of class `name`: return its static field
/// variables; static methods see the class as `this`. An inherited static
/// method that uses `this` is copied into the class when the class is
/// exported or the module names it through the class (`C.m`); no other
/// call can reach it.
pub(super) fn lower_class(
    class: &mut ast::Class,
    (name, exported): (&str, bool),
    classes: &Classes,
    references: &BTreeSet<(String, String)>,
    source: &Source,
) -> Result<Vec<ast::Decl>, CompilerError> {
    let Some(statics) = classes.get(name) else {
        return Ok(Vec::new());
    };
    let mut variables = Vec::new();
    let mut body = Vec::with_capacity(class.body.len());
    for member in std::mem::take(&mut class.body) {
        match static_accessor(&member).into_owned() {
            ast::ClassMember::ClassProp(property) if property.is_static => {
                variables.push(variable(property, name, source)?);
            }
            ast::ClassMember::StaticBlock(block) => {
                return Err(source.locate(
                    unsupported(
                        "static class block (code that runs at class definition needs the module initialization design)",
                    ),
                    block.span,
                ));
            }
            ast::ClassMember::Method(mut method) if method.is_static => {
                this_to_class(&mut method.function, name, source)?;
                body.push(ast::ClassMember::Method(method));
            }
            member => body.push(member),
        }
    }
    // Inherited static methods that use `this` run with this class as `this`.
    for inherited in &statics.this_methods {
        let declared = body.iter().any(|member| {
            matches!(member, ast::ClassMember::Method(own)
                if own.is_static && key(&own.key) == key(&inherited.key))
        });
        let reached = key(&inherited.key)
            .is_some_and(|method| exported || references.contains(&(name.to_owned(), method)));
        if declared || !reached {
            continue;
        }
        let mut copy = inherited.clone();
        if copy.accessibility == Some(ast::Accessibility::Private) {
            return Err(source.locate(
                unsupported("private static method that uses this, inherited by a subclass"),
                copy.span,
            ));
        }
        this_to_class(&mut copy.function, name, source)?;
        body.push(ast::ClassMember::Method(copy));
    }
    class.body = body;
    Ok(variables)
}

/// `static x: T = v` as the module variable `let __vartaDecl$C$x: T = v`.
fn variable(
    property: ast::ClassProp,
    class: &str,
    source: &Source,
) -> Result<ast::Decl, CompilerError> {
    let Some(field) = key(&property.key) else {
        return Err(source.locate(
            unsupported("computed static class field name"),
            property.key.span(),
        ));
    };
    if !property.decorators.is_empty() {
        return Err(source.locate(unsupported("static field decorator"), property.span));
    }
    if property.declare || property.is_abstract {
        return Err(source.locate(
            type_error("a static field cannot be declare or abstract"),
            property.span,
        ));
    }
    let mut value = property.value;
    if let Some(value) = &mut value {
        let mut rewrite = ThisToClass {
            name: class,
            error: None,
        };
        value.visit_mut_with(&mut rewrite);
        if let Some(span) = rewrite.error {
            return Err(source.locate(unsupported("super in static class code"), span));
        }
    }
    let mut type_ann = property.type_ann;
    if property.is_optional
        && let Some(annotation) = &mut type_ann
    {
        let span = annotation.span;
        let inner = std::mem::replace(
            &mut annotation.type_ann,
            Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
                span,
                kind: ast::TsKeywordTypeKind::TsUndefinedKeyword,
            })),
        );
        *annotation.type_ann = ast::TsType::TsUnionOrIntersectionType(
            ast::TsUnionOrIntersectionType::TsUnionType(ast::TsUnionType {
                span,
                types: vec![
                    inner,
                    Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
                        span,
                        kind: ast::TsKeywordTypeKind::TsUndefinedKeyword,
                    })),
                ],
            }),
        );
    }
    let span = property.span;
    Ok(ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Let,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(ast::BindingIdent {
                id: ast::Ident::new_no_ctxt(
                    generated(&[class, &field]).into(),
                    property.key.span(),
                ),
                type_ann,
            }),
            init: value,
            definite: false,
        }],
    })))
}
