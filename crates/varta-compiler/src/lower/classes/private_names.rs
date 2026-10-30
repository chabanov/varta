//! ES private names (v37 `class_generics`). A class body's `#x` members and
//! every `obj.#x` reference inside that body are rewritten, before layout, to
//! ordinary `private` members named `#x`. Public names cannot start with `#`
//! (quoted `'#x'` member names and computed `obj['#x']` accesses of class
//! members are rejected under the profile), so `#x` never collides with a
//! public member, and the class-scoped `private` check is exactly the private
//! name's lexical scope: class declarations are top-level, so every `#x` in
//! the body names the class's own member. The brand check `#x in obj` is
//! lowered as the class test of the declaring class.
use super::*;
use swc_ecma_visit::{VisitMut, VisitMutWith};

/// The prefix of a rewritten private member name (and of v37 private state).
pub(in crate::lower) const PREFIX: char = varta_common::value::PRIVATE_KEY_PREFIX;

pub(in crate::lower) fn is_private_name(name: &str) -> bool {
    name.starts_with(PREFIX)
}

fn private_key(name: &ast::PrivateName) -> ast::PropName {
    ast::PropName::Ident(ast::IdentName::new(
        format!("{PREFIX}{}", name.name).into(),
        name.span,
    ))
}

fn quoted_private(key: &ast::PropName) -> Option<Span> {
    match key {
        ast::PropName::Str(text) if text.value.as_str().is_some_and(is_private_name) => {
            Some(text.span)
        }
        _ => None,
    }
}

/// The member rewritten to an ordinary `private` member, or the unsupported
/// form's diagnostic.
fn member(member: ast::ClassMember, source: &Source) -> Result<ast::ClassMember> {
    let accessibility = |given: Option<ast::Accessibility>, span: Span| {
        if given.is_some() {
            return Err(source.locate(
                unsupported("accessibility modifier on a #private member"),
                span,
            ));
        }
        Ok(Some(ast::Accessibility::Private))
    };
    Ok(match member {
        ast::ClassMember::PrivateProp(p) if p.is_static => {
            return Err(source.locate(unsupported("static #private class field"), p.span));
        }
        ast::ClassMember::PrivateMethod(p) if p.is_static => {
            return Err(source.locate(unsupported("static #private class method"), p.span));
        }
        ast::ClassMember::PrivateMethod(p) if p.kind == ast::MethodKind::Setter => {
            return Err(source.locate(unsupported("#private class setter"), p.span));
        }
        ast::ClassMember::PrivateProp(p) => ast::ClassMember::ClassProp(ast::ClassProp {
            span: p.span,
            key: private_key(&p.key),
            value: p.value,
            type_ann: p.type_ann,
            is_static: false,
            decorators: p.decorators,
            accessibility: accessibility(p.accessibility, p.span)?,
            is_abstract: false,
            is_optional: p.is_optional,
            is_override: p.is_override,
            readonly: p.readonly,
            declare: false,
            definite: p.definite,
        }),
        ast::ClassMember::PrivateMethod(p) => ast::ClassMember::Method(ast::ClassMethod {
            span: p.span,
            key: private_key(&p.key),
            function: p.function,
            kind: p.kind,
            is_static: false,
            accessibility: accessibility(p.accessibility, p.span)?,
            is_abstract: p.is_abstract,
            is_optional: p.is_optional,
            is_override: p.is_override,
        }),
        ast::ClassMember::ClassProp(p) if quoted_private(&p.key).is_some() => {
            return Err(source.locate(
                unsupported("quoted class member named like a #private name"),
                p.span,
            ));
        }
        ast::ClassMember::Method(m) if quoted_private(&m.key).is_some() => {
            return Err(source.locate(
                unsupported("quoted class member named like a #private name"),
                m.span,
            ));
        }
        other => other,
    })
}

/// Rewrites `obj.#x` member properties inside the class body.
struct References;
impl VisitMut for References {
    fn visit_mut_member_prop(&mut self, prop: &mut ast::MemberProp) {
        if let ast::MemberProp::PrivateName(name) = prop {
            *prop = ast::MemberProp::Ident(ast::IdentName::new(
                format!("{PREFIX}{}", name.name).into(),
                name.span,
            ));
        }
        prop.visit_mut_children_with(self);
    }
}

/// The class declaration with private members and references rewritten.
pub(super) fn rewrite(class: &ast::Class, source: &Source) -> Result<Box<ast::Class>> {
    let mut class = class.clone();
    class.body = std::mem::take(&mut class.body)
        .into_iter()
        .map(|m| member(m, source))
        .collect::<Result<_>>()?;
    class.body.visit_mut_with(&mut References);
    Ok(Box::new(class))
}

/// Whether a value of `ty` may reach an instance of a class with a private
/// field. Private fields are not properties in JavaScript; the host
/// serializer and readers would see them as slots, so such values are kept
/// out of `JSON.stringify` and `unknown` at compile time.
pub(in crate::lower) fn reaches_private(ty: Type, types: &ir::ValueTypes) -> bool {
    types.profile.class_generics()
        && error_classes::reaches_class(ty, types, &|id| {
            types
                .classes
                .get(id)
                .fields
                .iter()
                .any(|field| is_private_name(&field.name))
        })
}

/// V37: instance fields that are not own enumerable properties in JavaScript
/// are internal state. After lowering, their descriptor keys take the
/// private-name prefix, so `json_stringify_v17` skips them like `#x` fields;
/// guest code reads every field by slot, never by key. They are
///
/// - the fields of engine prelude classes (a `Headers` or `AbortSignal` has
///   no own property), and
/// - the Error slots of class instances: `message` and `cause` are
///   non-enumerable own properties, and `name` lives on the prototype unless
///   a constructor in the class chain assigns `this.name`.
pub(in crate::lower) fn hide_prelude_state(types: &mut ir::ValueTypes) {
    if !types.profile.class_generics() {
        return;
    }
    let classes = &types.classes;
    for record in &mut types.records {
        let Some(class) = record.class else {
            continue;
        };
        let declared = &classes.get(class).fields;
        for (field, declared) in record.fields.iter_mut().zip(declared) {
            if is_private_name(&field.name) {
                continue;
            }
            // A prelude instance has no own property; a subclass field
            // redeclaring an Error slot is one.
            let hidden = if classes.get(class).prelude {
                true
            } else if classes.root_family(declared.owner).is_some() {
                declared.redeclared.is_none()
                    && (field.name != error_classes::NAME || !assigns_name(class, classes))
            } else {
                classes.get(declared.owner).prelude
            };
            if hidden {
                field.name = format!("{PREFIX}{}", field.name);
            }
        }
    }
}

/// Whether `class` or an ancestor assigns `this.name` in a top-level
/// statement of its constructor, making `name` an own enumerable property.
fn assigns_name(class: u32, classes: &Classes) -> bool {
    let mut current = Some(class);
    while let Some(id) = current {
        // A prelude class keeps `name` on its prototype (`DOMException`).
        if classes.get(id).prelude {
            return false;
        }
        let declaration = &classes.get(id).declaration;
        let assigns = declaration.body.iter().any(|member| {
            let ast::ClassMember::Constructor(constructor) = member else {
                return false;
            };
            constructor
                .body
                .iter()
                .flat_map(|body| &body.stmts)
                .any(|statement| {
                    let ast::Stmt::Expr(statement) = statement else {
                        return false;
                    };
                    let ast::Expr::Assign(assign) = &*statement.expr else {
                        return false;
                    };
                    matches!(
                        &assign.left,
                        ast::AssignTarget::Simple(ast::SimpleAssignTarget::Member(member))
                            if matches!(&*member.obj, ast::Expr::This(_))
                                && matches!(&member.prop, ast::MemberProp::Ident(prop)
                                    if prop.sym == error_classes::NAME)
                    )
                })
        });
        if assigns {
            return true;
        }
        current = classes.get(id).parent;
    }
    false
}

impl Lower<'_> {
    /// `#x in value`: the brand test of the class whose body declares `#x`.
    pub(in crate::lower) fn private_in(
        &mut self,
        name: &ast::PrivateName,
        right: &ast::Expr,
        span: Span,
    ) -> Result<Expr> {
        let class = self.private_owner(name)?;
        let value = self.expr(right)?;
        // TypeScript: the right operand of `in` must not be a primitive (in
        // JavaScript the test throws a TypeError).
        let object = |ty: &Type| {
            matches!(
                ty,
                Type::Record(_)
                    | Type::Class(_)
                    | Type::Array(_)
                    | Type::Dictionary(_)
                    | Type::Function(_)
                    | Type::Promise(_)
            )
        };
        if !unions::members(value.ty, self.types).iter().all(object) {
            return Err(self.source.locate(
                type_error(format!(
                    "the right operand of `{PREFIX}{} in` must be an object type, found {}",
                    name.name, value.ty
                )),
                value.span,
            ));
        }
        self.class_test(value, class, span)
    }
    /// The enclosing class, which must declare `#x` itself.
    pub(in crate::lower) fn private_owner(&self, name: &ast::PrivateName) -> Result<u32> {
        let key = format!("{PREFIX}{}", name.name);
        let declared = self.types.classes.context.map(|c| c.class).filter(|class| {
            let info = self.types.classes.get(*class);
            info.fields
                .iter()
                .any(|f| f.name == key && f.owner == *class)
                || info.methods.get(&key).is_some_and(|m| m.owner == *class)
                || info
                    .generic_methods
                    .get(&key)
                    .is_some_and(|m| m.owner == *class)
        });
        declared.ok_or_else(|| {
            self.source.locate(
                invalid(format!(
                    "private name {key} is not declared by the enclosing class"
                )),
                name.span,
            )
        })
    }
}
