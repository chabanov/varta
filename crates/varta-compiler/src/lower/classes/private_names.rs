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

/// The prefix of a rewritten private member name.
pub(in crate::lower) const PREFIX: char = '#';

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
