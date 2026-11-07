//! Index signatures of pattern key types (TypeScript 4.4, `{ [key:
//! `on${string}`]: T }`, `Record<`on${string}`, T>` and mapped types over a
//! pattern): a dictionary whose static view (`ir::MixedMembers::keys`) admits
//! only the keys the pattern describes, besides its declared fields. The
//! storage is the plain dictionary's, so the restriction is static, as in
//! tsc: a property name that no pattern matches does not exist on the type
//! (reads, writes, `delete`, object literal properties), and a computed key
//! must have a type assignable to the key type. The plain `Record<string,
//! T>` and the keyed type convert into each other freely.
use super::*;
use ir::LiteralBase;
use literals::LiteralValue;

/// The pattern key type of a keyed dictionary type.
pub(in crate::lower) fn key_type(ty: Type, types: &ir::ValueTypes) -> Option<Type> {
    types
        .declared(ty)?
        .keys
        .map(|set| Type::Literal(LiteralBase::String, set))
}

/// Whether the property `name` exists on `ty`: any name of a plain
/// dictionary, and on a keyed one a declared field or a value of its key
/// type.
pub(in crate::lower) fn admits(ty: Type, name: &str, types: &ir::ValueTypes) -> bool {
    let Some(members) = types.declared(ty) else {
        return true;
    };
    let Some(keys) = members.keys else {
        return true;
    };
    if members.fields.iter().any(|field| field.name == name) {
        return true;
    }
    let units: Vec<u16> = name.encode_utf16().collect();
    let value = literals::string_value(&units, types.profile);
    matches!(value, LiteralValue::String(_)) && value_in(&value, types.literals.values(keys), types)
}

/// The pattern members of a key type (the literal set of an index
/// signature's key type) and the other members (property names).
pub(super) fn split_patterns(
    keys: Type,
    types: &mut ir::ValueTypes,
) -> Result<(Option<u32>, Type)> {
    let mut patterns = std::collections::BTreeSet::new();
    let mut others = Vec::new();
    for member in split(keys, types) {
        match member {
            Type::Literal(LiteralBase::String, id) if types.literals.has_pattern(id) => {
                patterns.extend(types.literals.values(id).iter().cloned());
            }
            other => others.push(other),
        }
    }
    let rest = union_of(others, types)?;
    if patterns.is_empty() {
        return Ok((None, rest));
    }
    match literals::of_values(LiteralBase::String, patterns, types)? {
        Type::Literal(_, set) => Ok((Some(set), rest)),
        _ => Err(invalid("missing pattern key type")),
    }
}

/// The keys of a keyed dictionary type for `keyof`: its declared field
/// names and its pattern key type.
pub(super) fn keys_of(ty: Type, types: &mut ir::ValueTypes) -> Result<Option<Type>> {
    let Some(pattern) = key_type(ty, types) else {
        return Ok(None);
    };
    let names: Vec<String> = types
        .declared(ty)
        .map(|members| {
            members
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect()
        })
        .unwrap_or_default();
    let mut members = vec![pattern];
    if !names.is_empty() {
        members.push(objects::string_keys(names, types)?);
    }
    union_of(members, types).map(Some)
}

/// A key type in a diagnostic.
fn describe_key(ty: Type, types: &ir::ValueTypes) -> String {
    match ty {
        Type::Literal(_, set) => literals::describe(set, types),
        other => subtyping::describe(other, types),
    }
}

/// A keyed dictionary type in a diagnostic: its name, or its index
/// signature.
fn describe(ty: Type, types: &ir::ValueTypes) -> String {
    if let Some(name) = named::declared_name(types, ty) {
        return name;
    }
    match (key_type(ty, types), ty) {
        (Some(keys), Type::Dictionary(id)) => format!(
            "{{ [key: {}]: {} }}",
            describe_key(keys, types),
            subtyping::describe(types.dictionaries[id as usize].value, types)
        ),
        _ => subtyping::describe(ty, types),
    }
}

impl Lower<'_> {
    /// Check the key of a member access `d.name` or `d[key]` on a keyed
    /// dictionary type (tsc: TS2339 for a name, TS7053 for a computed key of
    /// another type); `key` is the lowered key.
    pub(in crate::lower) fn index_key(
        &self,
        ty: Type,
        member: &ast::MemberExpr,
        key: &Expr,
    ) -> Result<()> {
        let Some(wanted) = key_type(ty, self.types) else {
            return Ok(());
        };
        if let Some(name) = mixed_records::static_key(member) {
            return self.literal_key(ty, &name, member.prop.span(), false);
        }
        let given = key.ty;
        if extends(given, wanted, self.types) {
            return Ok(());
        }
        Err(self.source.locate(
            type_error(format!(
                "an expression of type {} cannot index type {}: it has no index signature with a \
                 parameter of that type",
                describe_key(given, self.types),
                describe(ty, self.types),
            )),
            member.prop.span(),
        ))
    }

    /// Check a property name of a keyed dictionary type: named in an access
    /// (TS2339), or by an object literal (`literal`, TS2353).
    pub(in crate::lower) fn literal_key(
        &self,
        ty: Type,
        name: &str,
        span: swc_common::Span,
        literal: bool,
    ) -> Result<()> {
        if admits(ty, name, self.types) {
            return Ok(());
        }
        let described = describe(ty, self.types);
        Err(self.source.locate(
            type_error(if literal {
                format!(
                    "object literal may only specify known properties, and {name} does not exist \
                     in type {described}"
                )
            } else {
                format!("property {name} does not exist on type {described}")
            }),
            span,
        ))
    }
}
