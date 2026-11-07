//! Class index signatures (v38 `declaration_forms`): `[key: string]: T` in a
//! class gives its instances own properties beyond the declared members, as
//! in JavaScript. They live in a hidden dictionary field (`INDEX_FIELD`, a
//! private name no source can spell) that the constructor creates empty; a
//! read, write, compound assignment or update of a member the class does not
//! declare (a name or a computed string key) is a dictionary operation on
//! it. Serialization lists the dictionary's entries as own properties in
//! own-key order ([`varta_common::value::INDEX_SIGNATURE_KEY`]).
use super::*;

/// The hidden dictionary field of a class with a string index signature.
pub(in crate::lower) const INDEX_FIELD: &str = varta_common::value::INDEX_SIGNATURE_KEY;

/// `[key: string]: T` as the private field `INDEX_FIELD: { [key: string]: T } = {}`.
pub(super) fn field(
    signature: &ast::TsIndexSignature,
    source: &Source,
) -> Result<ast::ClassMember> {
    let span = signature.span;
    let refused = |kind: &str| source.locate(unsupported(kind), span);
    if signature.is_static {
        return Err(refused("static class index signature"));
    }
    if signature.readonly {
        return Err(refused("readonly class index signature"));
    }
    let [ast::TsFnParam::Ident(key)] = signature.params.as_slice() else {
        return Err(refused(
            "class index signature with a parameter other than a key",
        ));
    };
    let string_key = matches!(
        key.type_ann.as_deref().map(|a| &*a.type_ann),
        Some(ast::TsType::TsKeywordType(ast::TsKeywordType {
            kind: ast::TsKeywordTypeKind::TsStringKeyword,
            ..
        }))
    );
    if !string_key {
        return Err(refused(
            "class index signature with a key other than string",
        ));
    }
    let Some(value) = signature.type_ann.clone() else {
        return Err(refused("class index signature without a value type"));
    };
    let dictionary = ast::TsType::TsTypeLit(ast::TsTypeLit {
        span,
        members: vec![ast::TsTypeElement::TsIndexSignature(
            ast::TsIndexSignature {
                params: signature.params.clone(),
                type_ann: Some(value),
                readonly: false,
                is_static: false,
                span,
            },
        )],
    });
    Ok(ast::ClassMember::ClassProp(ast::ClassProp {
        span,
        key: ast::PropName::Ident(ast::IdentName::new(INDEX_FIELD.into(), span)),
        value: Some(Box::new(ast::Expr::Object(ast::ObjectLit {
            span,
            props: Vec::new(),
        }))),
        type_ann: Some(Box::new(ast::TsTypeAnn {
            span,
            type_ann: Box::new(dictionary),
        })),
        is_static: false,
        decorators: Vec::new(),
        accessibility: Some(ast::Accessibility::Private),
        is_abstract: false,
        is_optional: false,
        is_override: false,
        readonly: true,
        declare: false,
        definite: false,
    }))
}

/// TS2411: every declared instance member of a class with an index
/// signature must be of the index's value type (methods and accessors are
/// properties of the instance's prototype, so they must be too).
pub(super) fn check_members(id: u32, source: &Source, types: &ir::ValueTypes) -> Result<()> {
    let class = types.classes.get(id);
    let Some((_, index)) = types.classes.field(id, INDEX_FIELD) else {
        return Ok(());
    };
    let Type::Dictionary(dictionary) = index.ty else {
        return Ok(());
    };
    let value = types.dictionaries[dictionary as usize].value;
    let mismatch = |name: &str, ty: &str| {
        source.locate(
            type_error(format!(
                "property {} of type {ty} is not assignable to the string index type {value} (TS2411)",
                forms::display_key(name)
            )),
            class.span,
        )
    };
    for field in &class.fields {
        if field.name == INDEX_FIELD || is_private_name(&field.name) {
            continue;
        }
        if !arguments::accepts(value, field.ty, types) {
            return Err(mismatch(&field.name, &field.ty.to_string()));
        }
    }
    if value != Type::Unknown
        && let Some((name, _)) = class
            .methods
            .iter()
            .find(|(name, _)| !is_private_name(name))
    {
        return Err(mismatch(name, "method or accessor"));
    }
    Ok(())
}

impl Lower<'_> {
    /// The index dictionary of a class instance for a member the class does
    /// not declare; otherwise `value` itself.
    pub(in crate::lower) fn class_index(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
    ) -> Result<Expr> {
        let Type::Class(class) = value.ty else {
            return Ok(value);
        };
        if !self.types.profile.declaration_forms() {
            return Ok(value);
        }
        let Some((slot, index)) = self.types.classes.field(class, INDEX_FIELD) else {
            return Ok(value);
        };
        let index_ty = index.ty;
        let declared = match &member.prop {
            ast::MemberProp::Ident(id) => Some(id.sym.to_string()),
            ast::MemberProp::Computed(key) => match &*key.expr {
                ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().map(str::to_owned),
                _ => None,
            },
            ast::MemberProp::PrivateName(_) => return Ok(value),
        };
        if let Some(name) = &declared {
            let info = self.types.classes.get(class);
            if info.fields.iter().any(|f| f.name == *name)
                || info.methods.contains_key(name)
                || info.methods.contains_key(&forms::setter_key(name))
                || info.generic_methods.contains_key(name)
                || is_private_name(name)
            {
                return Ok(value);
            }
        } else if let ast::MemberProp::Computed(key) = &member.prop
            && well_known_symbol(&key.expr, true).is_some()
        {
            return Ok(value);
        }
        Ok(Expr {
            span: value.span,
            ty: index_ty,
            kind: E::Field(Box::new(value), slot),
        })
    }
}
