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
    /// The exported function another module writes the field through
    /// (imported module variables are read-only bindings).
    pub setter: Option<String>,
}

/// The name segment of a static block's generated module variable.
pub(super) const BLOCK: &str = "static";

/// The static members of one module-level class.
#[derive(Clone, Default)]
pub(super) struct Statics {
    pub parent: Option<String>,
    /// Own and inherited fields: name, field, generated variable.
    pub fields: Vec<(String, Field, String)>,
    /// Static methods that use `this`, own and inherited, as declared, with
    /// their declaring (home) class.
    this_methods: Vec<(String, ast::ClassMethod)>,
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

/// Whether a function's own code uses `this` or `super` (outside nested
/// non-arrow functions).
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
    // `super.m()` calls `m` with this class as `this`.
    fn visit_super_prop_expr(&mut self, _: &ast::SuperPropExpr) {
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
            .filter(|(_, method)| {
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
                        setter: (!property.readonly && field_type(property).is_some())
                            .then(|| generated(&[name, &field, "set"])),
                    },
                    generated(&[name, &field]),
                ));
            }
            ast::ClassMember::Method(method) if method.is_static && uses_this(&method.function) => {
                let method_name = key(&method.key);
                statics
                    .this_methods
                    .retain(|(_, m)| key(&m.key) != method_name);
                statics.this_methods.push((name.to_owned(), method.clone()));
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

/// The static method of `home` that `super.m()` reaches with a subclass as
/// `this`, copied into that subclass under this name.
fn super_key(home: &str, method: &str) -> String {
    generated(&["super", home, method])
}

/// Replace `this` (outside nested non-arrow functions) by the class name,
/// and `super.x` by the home class's parent: `P.x`, or for a static method
/// of `P` that uses `this`, the copy of it that runs with this class as
/// `this` (recorded in `supers`).
struct ThisToClass<'a> {
    name: &'a str,
    /// The parent of the class whose code this is, and its static methods
    /// that use `this`.
    parent: Option<(&'a str, &'a [(String, ast::ClassMethod)])>,
    supers: Vec<(String, ast::ClassMethod)>,
    error: Option<Span>,
}
impl<'a> ThisToClass<'a> {
    fn new(name: &'a str, home: &str, classes: Option<&'a Classes>) -> Self {
        let parent = classes.and_then(|classes| {
            let parent = classes.get(home)?.parent.as_deref()?;
            let methods = classes.get(parent)?.this_methods.as_slice();
            Some((parent, methods))
        });
        Self {
            name,
            parent,
            supers: Vec::new(),
            error: None,
        }
    }
    fn super_member(&mut self, property: &ast::SuperPropExpr) -> Option<ast::Expr> {
        let ast::SuperProp::Ident(member) = &property.prop else {
            return None;
        };
        let (parent, methods) = self.parent?;
        let span = property.span;
        let ident = |name: &str| ast::Expr::Ident(ast::Ident::new_no_ctxt(name.into(), span));
        let found = methods
            .iter()
            .find(|(_, m)| key(&m.key).as_deref() == Some(member.sym.as_ref()));
        let (base, name) = match found {
            Some((home, method)) => {
                if !self
                    .supers
                    .iter()
                    .any(|(h, m)| h == home && m.key == method.key)
                {
                    self.supers.push((home.clone(), method.clone()));
                }
                (self.name.to_owned(), super_key(home, member.sym.as_ref()))
            }
            None => (parent.to_owned(), member.sym.to_string()),
        };
        Some(ast::Expr::Member(ast::MemberExpr {
            span,
            obj: Box::new(ident(&base)),
            prop: ast::MemberProp::Ident(ast::IdentName::new(name.into(), member.span)),
        }))
    }
}
impl VisitMut for ThisToClass<'_> {
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        match expr {
            ast::Expr::This(this) => {
                *expr = ast::Expr::Ident(ast::Ident::new_no_ctxt(self.name.into(), this.span));
            }
            ast::Expr::SuperProp(property) => match self.super_member(property) {
                Some(replacement) => *expr = replacement,
                None => {
                    self.error.get_or_insert(property.span);
                }
            },
            _ => expr.visit_mut_children_with(self),
        }
    }
    fn visit_mut_function(&mut self, _: &mut ast::Function) {}
    fn visit_mut_class(&mut self, _: &mut ast::Class) {}
    fn visit_mut_getter_prop(&mut self, _: &mut ast::GetterProp) {}
    fn visit_mut_setter_prop(&mut self, _: &mut ast::SetterProp) {}
}

/// Static code of class `home` run with class `name` as `this`; returns
/// the parent's static methods its `super` calls need copied.
fn this_to_class(
    function: &mut ast::Function,
    (name, home): (&str, &str),
    classes: Option<&Classes>,
    source: &Source,
) -> Result<Vec<(String, ast::ClassMethod)>, CompilerError> {
    let mut rewrite = ThisToClass::new(name, home, classes);
    function.params.visit_mut_with(&mut rewrite);
    if let Some(body) = &mut function.body {
        body.visit_mut_with(&mut rewrite);
    }
    match rewrite.error {
        Some(span) => Err(source.locate(
            unsupported(
                "super in static code outside a module-level subclass, or a computed super member",
            ),
            span,
        )),
        None => Ok(rewrite.supers),
    }
}

/// The key of the receiver version of the static method `method` (v38
/// class values): no source member name can spell it.
pub(crate) fn receiver_key(method: &str) -> String {
    generated(&[RECEIVER, method])
}
/// The name segment of receiver versions and the name of their receiver
/// parameter.
const RECEIVER: &str = "this";

/// `this` in static code as the receiver parameter, except for static
/// field accesses `this.x`, which name the declaring class's field, and
/// `super.m(…)`, which calls the parent's method with the same receiver
/// (its receiver version when it uses `this`).
struct ThisToReceiver<'a> {
    class: &'a str,
    fields: BTreeSet<&'a str>,
    receiver: &'a str,
    /// The parent class and its static methods that use `this`.
    parent: Option<(&'a str, &'a [(String, ast::ClassMethod)])>,
    error: Option<Span>,
}
impl ThisToReceiver<'_> {
    fn ident(&self, name: &str, span: Span) -> ast::Expr {
        ast::Expr::Ident(ast::Ident::new_no_ctxt(name.into(), span))
    }
    /// `super.m` as `Parent.m`, or for a call of a `this`-using `m` the
    /// parent's receiver version (true: the receiver is passed).
    fn super_member(&self, property: &ast::SuperPropExpr, call: bool) -> Option<(ast::Expr, bool)> {
        let ast::SuperProp::Ident(member) = &property.prop else {
            return None;
        };
        let (parent, methods) = self.parent?;
        let span = property.span;
        let receiver = call
            && methods
                .iter()
                .any(|(_, method)| key(&method.key).as_deref() == Some(member.sym.as_ref()));
        let name = if receiver {
            receiver_key(member.sym.as_ref())
        } else {
            member.sym.to_string()
        };
        Some((
            ast::Expr::Member(ast::MemberExpr {
                span,
                obj: Box::new(self.ident(parent, span)),
                prop: ast::MemberProp::Ident(ast::IdentName::new(name.into(), member.span)),
            }),
            receiver,
        ))
    }
}
impl VisitMut for ThisToReceiver<'_> {
    fn visit_mut_expr(&mut self, expr: &mut ast::Expr) {
        match expr {
            ast::Expr::This(this) => {
                *expr = self.ident(self.receiver, this.span);
            }
            ast::Expr::Call(call)
                if matches!(&call.callee, ast::Callee::Expr(callee)
                    if matches!(&**callee, ast::Expr::SuperProp(_))) =>
            {
                call.args.visit_mut_with(self);
                let ast::Callee::Expr(callee) = &call.callee else {
                    return;
                };
                let ast::Expr::SuperProp(property) = &**callee else {
                    return;
                };
                match self.super_member(property, true) {
                    Some((target, receiver)) => {
                        if receiver {
                            let span = call.span;
                            call.args.insert(
                                0,
                                ast::ExprOrSpread {
                                    spread: None,
                                    expr: Box::new(self.ident(self.receiver, span)),
                                },
                            );
                        }
                        call.callee = ast::Callee::Expr(Box::new(target));
                    }
                    None => {
                        self.error.get_or_insert(property.span);
                    }
                }
            }
            ast::Expr::SuperProp(property) => match self.super_member(property, false) {
                Some((target, _)) => *expr = target,
                None => {
                    self.error.get_or_insert(property.span);
                }
            },
            _ => expr.visit_mut_children_with(self),
        }
    }
    // `this.x` of a static field, read or assigned.
    fn visit_mut_member_expr(&mut self, member: &mut ast::MemberExpr) {
        if matches!(&*member.obj, ast::Expr::This(_))
            && matches!(&member.prop, ast::MemberProp::Ident(field)
                if self.fields.contains(field.sym.as_ref()))
        {
            let span = member.obj.span();
            *member.obj = self.ident(self.class, span);
            return;
        }
        member.visit_mut_children_with(self);
    }
    fn visit_mut_function(&mut self, _: &mut ast::Function) {}
    fn visit_mut_class(&mut self, _: &mut ast::Class) {}
    fn visit_mut_getter_prop(&mut self, _: &mut ast::GetterProp) {}
    fn visit_mut_setter_prop(&mut self, _: &mut ast::SetterProp) {}
}

/// The key of the marker of a static method that uses `this` and has no
/// receiver version (v38): a subclass without its own copy cannot call it.
pub(crate) fn unshared_key(method: &str) -> String {
    generated(&[UNSHARED, method])
}
/// The name segment of [`unshared_key`].
const UNSHARED: &str = "unshared";

/// `static [unshared]m(): void {}`, which marks the method for lowering.
fn unshared_marker(name: &str, span: Span) -> ast::ClassMember {
    ast::ClassMember::Method(ast::ClassMethod {
        span,
        key: ast::PropName::Ident(ast::IdentName::new(unshared_key(name).into(), span)),
        function: Box::new(ast::Function {
            span,
            body: Some(ast::FunctionBody {
                span,
                stmts: Vec::new(),
            }),
            ..Default::default()
        }),
        kind: ast::MethodKind::Method,
        is_static: true,
        accessibility: Some(ast::Accessibility::Private),
        is_abstract: false,
        is_optional: false,
        is_override: false,
    })
}

/// Whether a static method's own code assigns or updates `this.x` for a
/// static field `x`.
fn writes_this_field(function: &ast::Function, fields: &BTreeSet<&str>) -> bool {
    struct Writes<'a>(&'a BTreeSet<&'a str>, bool);
    impl Writes<'_> {
        fn target(&mut self, member: &ast::MemberExpr) {
            if matches!(&*member.obj, ast::Expr::This(_))
                && matches!(&member.prop, ast::MemberProp::Ident(field)
                    if self.0.contains(field.sym.as_ref()))
            {
                self.1 = true;
            }
        }
    }
    impl Visit for Writes<'_> {
        fn visit_assign_expr(&mut self, assign: &ast::AssignExpr) {
            if let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member)) = &assign.left
            {
                self.target(member);
            }
            assign.visit_children_with(self);
        }
        fn visit_update_expr(&mut self, update: &ast::UpdateExpr) {
            if let ast::Expr::Member(member) = &*update.arg {
                self.target(member);
            }
            update.visit_children_with(self);
        }
        fn visit_function(&mut self, _: &ast::Function) {}
        fn visit_class(&mut self, _: &ast::Class) {}
    }
    let mut writes = Writes(fields, false);
    function.params.visit_with(&mut writes);
    function.body.visit_with(&mut writes);
    writes.1
}

/// The receiver version of a static method that uses `this` (v38 class
/// values): `static [this]m(this$: typeof C, …)` with `this` as that class
/// object. A subclass without a copy of the method (one declared in
/// another module, whose copy could not reach this module's private names,
/// or one this module never names) and a class object that may hold any
/// subclass call it with their own class. Static field accesses `this.x`
/// read `C.x`. `None` when `super` reaches no module-level parent.
fn receiver_version(
    method: &ast::ClassMethod,
    class: &str,
    (statics, classes): (&Statics, &Classes),
) -> Option<ast::ClassMember> {
    let name = key(&method.key)?;
    let span = method.span;
    let receiver = generated(&[RECEIVER]);
    let fields: BTreeSet<&str> = statics
        .fields
        .iter()
        .map(|(field, ..)| field.as_str())
        .collect();
    // Writing `this.x` through a subclass would create an own static
    // property of that subclass, which a static field variable cannot
    // model: such a method has no receiver version, only the marker.
    if writes_this_field(&method.function, &fields) {
        return Some(unshared_marker(&name, span));
    }
    let mut copy = method.clone();
    copy.key = ast::PropName::Ident(ast::IdentName::new(receiver_key(&name).into(), span));
    let parent = statics.parent.as_deref().and_then(|parent| {
        let methods = classes.get(parent)?.this_methods.as_slice();
        Some((parent, methods))
    });
    let mut rewrite = ThisToReceiver {
        class,
        fields,
        receiver: &receiver,
        parent,
        error: None,
    };
    copy.function.params.visit_mut_with(&mut rewrite);
    if let Some(body) = &mut copy.function.body {
        body.visit_mut_with(&mut rewrite);
    }
    if rewrite.error.is_some() {
        return Some(unshared_marker(&name, span));
    }
    let annotation = ast::TsType::TsTypeQuery(ast::TsTypeQuery {
        span,
        expr_name: ast::TsTypeQueryExpr::TsEntityName(ast::TsEntityName::Ident(
            ast::Ident::new_no_ctxt(class.into(), span),
        )),
        type_args: None,
    });
    copy.function.params.insert(
        0,
        ast::Param {
            span,
            decorators: Vec::new(),
            pat: ast::Pat::Ident(ast::BindingIdent {
                id: ast::Ident::new_no_ctxt(receiver.into(), span),
                type_ann: Some(Box::new(ast::TsTypeAnn {
                    span,
                    type_ann: Box::new(annotation),
                })),
            }),
        },
    );
    Some(ast::ClassMember::Method(copy))
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
    let mut supers = Vec::new();
    for member in std::mem::take(&mut class.body) {
        match static_accessor(&member).into_owned() {
            ast::ClassMember::ClassProp(property) if property.is_static => {
                let setter = exported.then(|| setter(&property, name)).flatten();
                variables.push(variable(property, name, classes, source)?);
                variables.extend(setter);
            }
            ast::ClassMember::StaticBlock(block) => {
                let index = variables.len();
                variables.push(static_block(block, name, index, classes, source)?);
            }
            ast::ClassMember::Method(mut method) if method.is_static => {
                if uses_this(&method.function) {
                    body.extend(receiver_version(&method, name, (statics, classes)));
                }
                supers.extend(this_to_class(
                    &mut method.function,
                    (name, name),
                    Some(classes),
                    source,
                )?);
                body.push(ast::ClassMember::Method(method));
            }
            member => body.push(member),
        }
    }
    // Inherited static methods that use `this` run with this class as `this`.
    for (home, inherited) in &statics.this_methods {
        if home == name {
            continue;
        }
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
        body.extend(receiver_version(inherited, name, (statics, classes)));
        supers.extend(this_to_class(
            &mut copy.function,
            (name, home),
            Some(classes),
            source,
        )?);
        body.push(ast::ClassMember::Method(copy));
    }
    // `super.m()` of a parent's static method that uses `this` runs a copy
    // of that method with this class as `this`; its own `super` calls
    // resolve from its declaring class.
    let mut copied: BTreeSet<String> = BTreeSet::new();
    while let Some((home, method)) = supers.pop() {
        let Some(method_name) = key(&method.key) else {
            continue;
        };
        let copy_name = super_key(&home, &method_name);
        if !copied.insert(copy_name.clone()) {
            continue;
        }
        let mut copy = method.clone();
        copy.key = ast::PropName::Ident(ast::IdentName::new(copy_name.into(), copy.key.span()));
        copy.accessibility = Some(ast::Accessibility::Private);
        supers.extend(this_to_class(
            &mut copy.function,
            (name, &home),
            Some(classes),
            source,
        )?);
        body.push(ast::ClassMember::Method(copy));
    }
    class.body = body;
    Ok(variables)
}

/// `static { … }` as the module variable
/// `let __vartaDecl$C$static$n: number = ((): number => { …; return 0; })();`,
/// evaluated in the module initializer sequence at the class's position
/// (v38 `module_evaluation`), with `this` the class.
fn static_block(
    block: ast::StaticBlock,
    class: &str,
    index: usize,
    classes: &Classes,
    source: &Source,
) -> Result<ast::Decl, CompilerError> {
    let span = block.span;
    let mut stmts = block.body.stmts;
    let mut rewrite = ThisToClass::new(class, class, Some(classes));
    stmts.visit_mut_with(&mut rewrite);
    if let Some(span) = rewrite.error {
        return Err(source.locate(unsupported("super in static class code"), span));
    }
    if !rewrite.supers.is_empty() {
        return Err(source.locate(
            unsupported("super call of a static method that uses this, from a static block"),
            span,
        ));
    }
    // The block's completion is a number: a module variable needs a value
    // with a boxed form.
    let number = || {
        Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
            span,
            kind: ast::TsKeywordTypeKind::TsNumberKeyword,
        }))
    };
    stmts.push(ast::Stmt::Return(ast::ReturnStmt {
        span,
        arg: Some(Box::new(ast::Expr::Lit(ast::Lit::Num(ast::Number {
            span,
            value: 0.0,
            raw: None,
        })))),
    }));
    let arrow = ast::Expr::Arrow(ast::ArrowExpr {
        span,
        ctxt: Default::default(),
        params: Vec::new(),
        body: Box::new(ast::ArrowFunctionBody::FunctionBody(ast::FunctionBody {
            span,
            stmts,
        })),
        is_async: false,
        is_generator: false,
        type_params: None,
        return_type: Some(Box::new(ast::TsTypeAnn {
            span,
            type_ann: number(),
        })),
    });
    let call = ast::Expr::Call(ast::CallExpr {
        span,
        ctxt: Default::default(),
        callee: ast::Callee::Expr(Box::new(ast::Expr::Paren(ast::ParenExpr {
            span,
            expr: Box::new(arrow),
        }))),
        args: Vec::new(),
        type_args: None,
    });
    let name = generated(&[class, BLOCK, &index.to_string()]);
    Ok(ast::Decl::Var(Box::new(ast::VarDecl {
        span,
        ctxt: Default::default(),
        kind: ast::VarDeclKind::Let,
        declare: false,
        decls: vec![ast::VarDeclarator {
            span,
            name: ast::Pat::Ident(ast::BindingIdent {
                id: ast::Ident::new_no_ctxt(name.into(), span),
                type_ann: Some(Box::new(ast::TsTypeAnn {
                    span,
                    type_ann: number(),
                })),
            }),
            init: Some(Box::new(call)),
            definite: false,
        }],
    })))
}

/// The declared type of a static field: its annotation, or the type of a
/// literal initializer.
fn field_type(property: &ast::ClassProp) -> Option<Box<ast::TsType>> {
    if let Some(annotation) = &property.type_ann {
        return (!property.is_optional).then(|| annotation.type_ann.clone());
    }
    let kind = match property.value.as_deref()? {
        ast::Expr::Lit(ast::Lit::Num(_)) => ast::TsKeywordTypeKind::TsNumberKeyword,
        ast::Expr::Lit(ast::Lit::Str(_)) => ast::TsKeywordTypeKind::TsStringKeyword,
        ast::Expr::Lit(ast::Lit::Bool(_)) => ast::TsKeywordTypeKind::TsBooleanKeyword,
        _ => return None,
    };
    Some(Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
        span: property.span,
        kind,
    })))
}

/// `function __vartaDecl$C$x$set(value: T): T { __vartaDecl$C$x = value; return value; }`,
/// through which importing modules write an exported class's static field.
fn setter(property: &ast::ClassProp, class: &str) -> Option<ast::Decl> {
    let field = key(&property.key)?;
    if property.readonly {
        return None;
    }
    let ty = field_type(property)?;
    let span = property.span;
    let ident = |name: &str| ast::Ident::new_no_ctxt(name.into(), span);
    let annotation = || {
        Some(Box::new(ast::TsTypeAnn {
            span,
            type_ann: ty.clone(),
        }))
    };
    let value = ident("value");
    let body = vec![
        ast::Stmt::Expr(ast::ExprStmt {
            span,
            expr: Box::new(ast::Expr::Assign(ast::AssignExpr {
                span,
                op: ast::AssignOp::Assign,
                left: ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(
                    ast::BindingIdent {
                        id: ident(&generated(&[class, &field])),
                        type_ann: None,
                    },
                )),
                right: Box::new(ast::Expr::Ident(value.clone())),
            })),
        }),
        ast::Stmt::Return(ast::ReturnStmt {
            span,
            arg: Some(Box::new(ast::Expr::Ident(value.clone()))),
        }),
    ];
    Some(ast::Decl::Fn(ast::FnDecl {
        ident: ident(&generated(&[class, &field, "set"])),
        declare: false,
        function: Box::new(ast::Function {
            params: vec![ast::Param {
                span,
                decorators: Vec::new(),
                pat: ast::Pat::Ident(ast::BindingIdent {
                    id: value,
                    type_ann: annotation(),
                }),
            }],
            span,
            body: Some(ast::FunctionBody { span, stmts: body }),
            return_type: annotation(),
            ..Default::default()
        }),
    }))
}

/// `static x: T = v` as the module variable `let __vartaDecl$C$x: T = v`.
fn variable(
    property: ast::ClassProp,
    class: &str,
    classes: &Classes,
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
        let mut rewrite = ThisToClass::new(class, class, Some(classes));
        value.visit_mut_with(&mut rewrite);
        if let Some(span) = rewrite.error {
            return Err(source.locate(unsupported("super in static class code"), span));
        }
        if !rewrite.supers.is_empty() {
            return Err(source.locate(
                unsupported(
                    "super call of a static method that uses this, from a static field initializer",
                ),
                property.span,
            ));
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
