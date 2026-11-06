//! Class declaration forms of v38 (`declaration_forms`), rewritten before
//! layout like `#private` names:
//!
//! - an abstract method or accessor gets a body that throws (it is never
//!   reached: an abstract class cannot be constructed and every concrete
//!   descendant must implement each abstract member, checked after layout);
//! - a setter `set x(v)` becomes the method named [`setter_key`]`("x")`,
//!   which assignments to `x` call;
//! - method and constructor overload signatures (body-less members) are
//!   removed and recorded for `lower::overloads`.
use super::*;

/// The method name a setter `set x(v)` is registered under. No member name
/// can start with it (`member_name` rejects the spelling).
pub(in crate::lower) const SETTER_PREFIX: &str = "[set] ";
/// The overload key of a constructor.
pub(in crate::lower) const CONSTRUCTOR_KEY: &str = CONSTRUCTOR_NAME;
/// The message of the unreachable body of an abstract member.
const ABSTRACT_BODY: &str = "abstract class member";

pub(in crate::lower) fn setter_key(name: &str) -> String {
    format!("{SETTER_PREFIX}{name}")
}

/// The source name of a member key (a setter's property name).
pub(in crate::lower) fn display_key(key: &str) -> &str {
    key.strip_prefix(SETTER_PREFIX).unwrap_or(key)
}

/// A body-less method or constructor declaration: an overload signature.
#[derive(Clone)]
pub(in crate::lower) struct OverloadDeclaration {
    pub key: String,
    pub is_static: bool,
    pub ident: ast::Ident,
    pub function: Box<ast::Function>,
}

#[derive(Default, Clone)]
pub(in crate::lower) struct Forms {
    pub is_abstract: bool,
    /// Keys of the abstract members the class declares.
    pub abstracts: BTreeSet<String>,
    pub overloads: Vec<OverloadDeclaration>,
    /// Whether the class declares a constructor implementation.
    pub constructor: bool,
}

fn key_name(key: &ast::PropName) -> Option<String> {
    match key {
        ast::PropName::Ident(id) => Some(id.sym.to_string()),
        ast::PropName::Str(text) => text.value.as_str().map(str::to_owned),
        _ => None,
    }
}

/// The diagnostic of a decorator: it runs code when its class is defined.
fn decorator(source: &Source, span: Span) -> CompilerError {
    source.locate(
        unsupported(
            "decorator (a decorator runs code when its class is defined, which needs the module initialization design)",
        ),
        span,
    )
}

/// The first decorator of a class or any of its members or parameters.
fn decorated(class: &ast::Class) -> Option<Span> {
    let first = |decorators: &[ast::Decorator]| decorators.first().map(|d| d.span);
    let function = |f: &ast::Function| {
        first(&f.decorators).or_else(|| f.params.iter().find_map(|p| first(&p.decorators)))
    };
    first(&class.decorators).or_else(|| {
        class.body.iter().find_map(|member| match member {
            ast::ClassMember::Method(m) => function(&m.function),
            ast::ClassMember::PrivateMethod(m) => function(&m.function),
            ast::ClassMember::ClassProp(p) => first(&p.decorators),
            ast::ClassMember::PrivateProp(p) => first(&p.decorators),
            ast::ClassMember::AutoAccessor(a) => first(&a.decorators),
            ast::ClassMember::Constructor(c) => c.params.iter().find_map(|p| match p {
                ast::ParamOrTsParamProp::Param(p) => first(&p.decorators),
                ast::ParamOrTsParamProp::TsParamProp(p) => first(&p.decorators),
            }),
            _ => None,
        })
    })
}

/// The private storage field of an auto-accessor; no source name can
/// spell it (a `#name` has no space).
fn accessor_storage(name: &str) -> String {
    format!("{}accessor {name}", private_names::PREFIX)
}

/// `accessor x: T = v` as a private storage field with a get/set pair, as
/// ECMAScript defines auto-accessors; a static one is a static field (the
/// class object exposes no property descriptors).
fn auto_accessor(accessor: ast::AutoAccessor, source: &Source) -> Result<Vec<ast::ClassMember>> {
    let span = accessor.span;
    let ast::Key::Public(key) = &accessor.key else {
        return Err(source.locate(unsupported("#private auto-accessor"), span));
    };
    let Some(name) = key_name(key) else {
        return Err(source.locate(unsupported("computed auto-accessor name"), span));
    };
    if accessor.is_static {
        return Ok(vec![ast::ClassMember::ClassProp(ast::ClassProp {
            span,
            key: key.clone(),
            value: accessor.value,
            type_ann: accessor.type_ann,
            is_static: true,
            decorators: Vec::new(),
            accessibility: accessor.accessibility,
            is_abstract: false,
            is_optional: false,
            is_override: accessor.is_override,
            readonly: false,
            declare: false,
            definite: accessor.definite,
        })]);
    }
    if accessor.is_abstract {
        return Err(source.locate(unsupported("abstract auto-accessor"), span));
    }
    let ty = match (&accessor.type_ann, accessor.value.as_deref()) {
        (Some(annotation), _) => annotation.type_ann.clone(),
        (None, Some(ast::Expr::Lit(literal))) => {
            let kind = match literal {
                ast::Lit::Num(_) => ast::TsKeywordTypeKind::TsNumberKeyword,
                ast::Lit::Str(_) => ast::TsKeywordTypeKind::TsStringKeyword,
                ast::Lit::Bool(_) => ast::TsKeywordTypeKind::TsBooleanKeyword,
                _ => return Err(accessor_type(source, span)),
            };
            Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
                span,
                kind,
            }))
        }
        _ => return Err(accessor_type(source, span)),
    };
    let storage = accessor_storage(&name);
    let this_storage = || ast::MemberExpr {
        span,
        obj: Box::new(ast::Expr::This(ast::ThisExpr { span })),
        prop: ast::MemberProp::Ident(ast::IdentName::new(storage.as_str().into(), span)),
    };
    let annotation = |ty: Box<ast::TsType>| Box::new(ast::TsTypeAnn { span, type_ann: ty });
    let field = ast::ClassMember::ClassProp(ast::ClassProp {
        span,
        key: ast::PropName::Ident(ast::IdentName::new(storage.as_str().into(), span)),
        value: accessor.value,
        type_ann: Some(annotation(ty.clone())),
        is_static: false,
        decorators: Vec::new(),
        accessibility: Some(ast::Accessibility::Private),
        is_abstract: false,
        is_optional: false,
        is_override: false,
        readonly: false,
        declare: false,
        definite: accessor.definite,
    });
    let getter = ast::ClassMember::Method(ast::ClassMethod {
        span,
        key: key.clone(),
        function: Box::new(ast::Function {
            span,
            body: Some(ast::FunctionBody {
                span,
                stmts: vec![ast::Stmt::Return(ast::ReturnStmt {
                    span,
                    arg: Some(Box::new(ast::Expr::Member(this_storage()))),
                })],
            }),
            return_type: Some(annotation(ty.clone())),
            ..Default::default()
        }),
        kind: ast::MethodKind::Getter,
        is_static: false,
        accessibility: accessor.accessibility,
        is_abstract: false,
        is_optional: false,
        is_override: accessor.is_override,
    });
    let value = ast::Ident::new_no_ctxt("value".into(), span);
    let setter = ast::ClassMember::Method(ast::ClassMethod {
        span,
        key: key.clone(),
        function: Box::new(ast::Function {
            span,
            params: vec![ast::Param {
                span,
                decorators: Vec::new(),
                pat: ast::Pat::Ident(ast::BindingIdent {
                    id: value.clone(),
                    type_ann: Some(annotation(ty)),
                }),
            }],
            body: Some(ast::FunctionBody {
                span,
                stmts: vec![ast::Stmt::Expr(ast::ExprStmt {
                    span,
                    expr: Box::new(ast::Expr::Assign(ast::AssignExpr {
                        span,
                        op: ast::AssignOp::Assign,
                        left: ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(
                            this_storage(),
                        )),
                        right: Box::new(ast::Expr::Ident(value)),
                    })),
                })],
            }),
            ..Default::default()
        }),
        kind: ast::MethodKind::Setter,
        is_static: false,
        accessibility: accessor.accessibility,
        is_abstract: false,
        is_optional: false,
        is_override: accessor.is_override,
    });
    Ok(vec![field, getter, setter])
}

fn accessor_type(source: &Source, span: Span) -> CompilerError {
    source.locate(
        type_error("an auto-accessor needs a type annotation or a literal initializer"),
        span,
    )
}

pub(super) fn rewrite(class: Box<ast::Class>, source: &Source) -> Result<(Box<ast::Class>, Forms)> {
    let mut class = class;
    if let Some(span) = decorated(&class) {
        return Err(decorator(source, span));
    }
    let mut forms = Forms {
        is_abstract: class.is_abstract,
        ..Forms::default()
    };
    let mut members = Vec::with_capacity(class.body.len());
    for member in std::mem::take(&mut class.body) {
        match member {
            ast::ClassMember::AutoAccessor(accessor) => {
                members.extend(auto_accessor(accessor, source)?);
            }
            member => members.push(member),
        }
    }
    let mut body = Vec::with_capacity(members.len());
    for member in members {
        match member {
            ast::ClassMember::Method(mut method) => {
                let Some(name) = key_name(&method.key) else {
                    body.push(ast::ClassMember::Method(method));
                    continue;
                };
                let key = if method.kind == ast::MethodKind::Setter && !method.is_static {
                    setter_key(&name)
                } else {
                    name.clone()
                };
                if method.is_abstract {
                    abstract_member(&mut method, &class, &name, source)?;
                    forms.abstracts.insert(key.clone());
                } else if method.function.body.is_none() {
                    // An overload signature.
                    forms.overloads.push(OverloadDeclaration {
                        key,
                        is_static: method.is_static,
                        ident: ast::Ident::new_no_ctxt(name.as_str().into(), method.key.span()),
                        function: method.function,
                    });
                    continue;
                }
                if method.kind == ast::MethodKind::Setter && !method.is_static {
                    method.kind = ast::MethodKind::Method;
                    method.key = ast::PropName::Ident(ast::IdentName::new(
                        key.as_str().into(),
                        method.key.span(),
                    ));
                    if method.function.return_type.is_some() {
                        return Err(source.locate(
                            type_error(
                                "a set accessor cannot have a return type annotation (TS1095)",
                            ),
                            method.span,
                        ));
                    }
                    method.function.return_type = Some(Box::new(void_annotation(method.span)));
                }
                body.push(ast::ClassMember::Method(method));
            }
            ast::ClassMember::Constructor(ctor) if ctor.body.is_none() => {
                let mut params = Vec::new();
                for param in &ctor.params {
                    match param {
                        ast::ParamOrTsParamProp::Param(param) => params.push(param.clone()),
                        ast::ParamOrTsParamProp::TsParamProp(prop) => {
                            return Err(source.locate(
                                type_error(
                                    "a parameter property is only allowed in a constructor implementation (TS2369)",
                                ),
                                prop.span,
                            ));
                        }
                    }
                }
                forms.overloads.push(OverloadDeclaration {
                    key: CONSTRUCTOR_KEY.to_owned(),
                    is_static: false,
                    ident: ast::Ident::new_no_ctxt(CONSTRUCTOR_NAME.into(), ctor.span),
                    function: Box::new(ast::Function {
                        params,
                        span: ctor.span,
                        return_type: Some(Box::new(void_annotation(ctor.span))),
                        ..Default::default()
                    }),
                });
            }
            ast::ClassMember::Constructor(ctor) => {
                forms.constructor = true;
                body.push(ast::ClassMember::Constructor(ctor));
            }
            ast::ClassMember::ClassProp(property) if property.is_abstract && !class.is_abstract => {
                return Err(abstract_outside(source, property.span));
            }
            // An abstract field defines nothing; a concrete descendant must.
            ast::ClassMember::ClassProp(mut property) if property.is_abstract => {
                if property.value.is_some() || property.is_static {
                    return Err(source.locate(
                        type_error(
                            "an abstract field cannot have an initializer or be static (TS1267)",
                        ),
                        property.span,
                    ));
                }
                if let Some(name) = key_name(&property.key) {
                    forms.abstracts.insert(name);
                }
                property.is_abstract = false;
                property.declare = true;
                body.push(ast::ClassMember::ClassProp(property));
            }
            member => body.push(member),
        }
    }
    class.body = body;
    Ok((class, forms))
}

fn abstract_outside(source: &Source, span: Span) -> CompilerError {
    source.locate(
        type_error("abstract members can only appear within an abstract class (TS1244)"),
        span,
    )
}

/// Give an abstract method its unreachable body.
fn abstract_member(
    method: &mut ast::ClassMethod,
    class: &ast::Class,
    name: &str,
    source: &Source,
) -> Result<()> {
    if !class.is_abstract {
        return Err(abstract_outside(source, method.span));
    }
    if method.function.body.is_some() {
        return Err(source.locate(
            type_error(format!(
                "method {name} cannot have an implementation because it is marked abstract (TS1245)"
            )),
            method.span,
        ));
    }
    if method.is_static || method.accessibility == Some(ast::Accessibility::Private) {
        return Err(source.locate(
            type_error("an abstract member cannot be static or private (TS1243)"),
            method.span,
        ));
    }
    let span = method.span;
    let error = ast::Expr::New(ast::NewExpr {
        span,
        ctxt: Default::default(),
        callee: Box::new(ast::Expr::Ident(ast::Ident::new_no_ctxt(
            "TypeError".into(),
            span,
        ))),
        args: Some(vec![ast::ExprOrSpread {
            spread: None,
            expr: Box::new(ast::Expr::Lit(ast::Lit::Str(ast::Str {
                span,
                value: ABSTRACT_BODY.into(),
                raw: None,
            }))),
        }]),
        type_args: None,
    });
    method.function.body = Some(ast::FunctionBody {
        span,
        stmts: vec![ast::Stmt::Throw(ast::ThrowStmt {
            span,
            arg: Box::new(error),
        })],
    });
    method.is_abstract = false;
    Ok(())
}

fn void_annotation(span: Span) -> ast::TsTypeAnn {
    ast::TsTypeAnn {
        span,
        type_ann: Box::new(ast::TsType::TsKeywordType(ast::TsKeywordType {
            span,
            kind: ast::TsKeywordTypeKind::TsVoidKeyword,
        })),
    }
}

/// After layout: a concrete class implements every abstract member it
/// inherits (TS2515), and an abstract class is never constructed (TS2511).
pub(super) fn check_concrete(id: u32, source: &Source, types: &ir::ValueTypes) -> Result<()> {
    let class = types.classes.get(id);
    if class.forms.is_abstract {
        return Ok(());
    }
    for field in &class.fields {
        let owner = types.classes.get(field.owner);
        if !owner.forms.abstracts.contains(&field.name) {
            continue;
        }
        // Some class between the declaring class and this one defines it.
        let mut current = Some(id);
        let mut defined = false;
        while let Some(c) = current.filter(|c| *c != field.owner) {
            let info = types.classes.get(c);
            defined |= info
                .initializers
                .iter()
                .any(|(name, _)| *name == field.name);
            current = info.parent;
        }
        if !defined {
            return Err(source.locate(
                type_error(format!(
                    "non-abstract class {} does not implement inherited abstract member {} from class {} (TS2515)",
                    class.name, field.name, owner.name
                )),
                class.span,
            ));
        }
    }
    for (key, method) in &class.methods {
        let owner = types.classes.get(method.owner);
        if owner.forms.abstracts.contains(key) {
            return Err(source.locate(
                type_error(format!(
                    "non-abstract class {} does not implement inherited abstract member {} from class {} (TS2515)",
                    class.name,
                    display_key(key),
                    owner.name
                )),
                class.span,
            ));
        }
    }
    Ok(())
}

/// After layout: a class that declares one half of an accessor pair
/// replaces the inherited pair as a whole in JavaScript, so a declared
/// getter without the inherited setter (or the reverse) would silently drop
/// it; such a class is rejected.
pub(super) fn check_accessor_pairs(id: u32, source: &Source, types: &ir::ValueTypes) -> Result<()> {
    let class = types.classes.get(id);
    for (key, method) in &class.methods {
        if method.owner != id {
            continue;
        }
        let (name, other) = match key.strip_prefix(SETTER_PREFIX) {
            Some(name) => (name.to_owned(), name.to_owned()),
            None if method.getter => (key.clone(), setter_key(key)),
            None => continue,
        };
        if let Some(inherited) = class.methods.get(&other)
            && inherited.owner != id
            && (other == name) == inherited.getter
        {
            return Err(source.locate(
                unsupported(&format!(
                    "accessor {name} that overrides only one half of an inherited get/set pair"
                )),
                class.span,
            ));
        }
    }
    Ok(())
}

impl Lower<'_> {
    /// The constructor overloads `new C(...)` resolves against: the own
    /// constructor's, or those of the nearest ancestor that declares one.
    pub(super) fn constructor_overloads(
        &self,
        class: u32,
    ) -> Option<std::rc::Rc<[super::super::overloads::Overload]>> {
        let mut current = Some(class);
        while let Some(id) = current {
            if let Some(overloads) = self.types.overloads.method(id, CONSTRUCTOR_KEY, false) {
                return Some(overloads);
            }
            let info = self.types.classes.get(id);
            if info.forms.constructor {
                return None;
            }
            current = info.parent;
        }
        None
    }
    /// A call of an overloaded method; `receiver` is absent for a static.
    pub(super) fn overloaded_class_call(
        &mut self,
        call: &ast::CallExpr,
        signature: &Signature,
        overloads: &[super::super::overloads::Overload],
        target: usize,
        receiver: Option<Expr>,
    ) -> Result<Expr> {
        if call.type_args.is_some() {
            return Err(unsupported("generic class method call"));
        }
        let (values, result) = self.overloaded_arguments(call, overloads, signature)?;
        let mut args: Vec<Expr> = receiver.into_iter().collect();
        args.extend(values);
        let value = Expr {
            span: call.span,
            ty: signature.result,
            kind: E::Call(target, args),
        };
        self.overloaded_result(value, result)
    }
    /// `new C(...)` of an abstract class is a compile error (TS2511).
    pub(super) fn reject_abstract_new(&self, class: u32, span: Span) -> Result<()> {
        if self.types.classes.get(class).forms.is_abstract {
            return Err(self.source.locate(
                type_error(format!(
                    "cannot create an instance of the abstract class {} (TS2511)",
                    self.types.classes.get(class).name
                )),
                span,
            ));
        }
        Ok(())
    }
    /// `super.m()` of an abstract method has no implementation (TS2513).
    pub(super) fn reject_abstract_super(
        &self,
        method: &Method,
        name: &str,
        span: Span,
    ) -> Result<()> {
        if self
            .types
            .classes
            .get(method.owner)
            .forms
            .abstracts
            .contains(name)
        {
            return Err(self.source.locate(
                type_error(format!(
                    "abstract method {} cannot be accessed via super (TS2513)",
                    display_key(name)
                )),
                span,
            ));
        }
        Ok(())
    }
}
