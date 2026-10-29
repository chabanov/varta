//! Mixed index records (v35 `mixed_index_records`): object types that declare
//! fields next to a string index signature, such as
//! `interface Problem { title?: string; status?: number; [key: string]: unknown }`.
//!
//! A mixed record is a dictionary of its index type `T`: the same storage
//! record, layout and value tag, so every key, declared or not, lives in one
//! entry list in ECMAScript own-key order and JSON, `Object.keys`, `in` and
//! `delete` behave exactly as for dictionaries. The declared fields are a
//! static view: a declared read looks its key up and converts the stored `T`
//! to the declared type with a checked test, because a dynamic-key write, an
//! alias typed as the plain dictionary or a `delete` may have changed the
//! entry. Writes of declared keys are typed by the declared field.
use super::*;
use ir::{DictionaryType, MixedMembers, RecordField};
use varta_common::value::ErrorClass;

/// Whether a value of `actual` may be stored where `expected` is declared.
pub(super) fn assignable(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    if expected == Type::Unknown {
        return unknown::admissible(actual);
    }
    arguments::accepts(expected, actual, types)
}

/// Whether `ty` admits `undefined` (an absent declared key then reads it).
pub(super) fn admits_undefined(ty: Type, types: &ir::ValueTypes) -> bool {
    ty == Type::Unknown || unions::members(ty, types).contains(&Type::Undefined)
}

/// Intern a mixed index record `{ fields; [key: string]: value }` or, with
/// `object`, the `object` view of `unknown` narrowed by `in` guards.
pub(super) fn intern(
    value: Type,
    mut fields: Vec<RecordField>,
    object: bool,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    if !object && !types.profile.mixed_index_records() {
        return Err(unsupported(
            "index signature combined with declared fields (a mixed object type)",
        ));
    }
    let Type::Dictionary(plain) = dictionaries::intern(value, types)? else {
        return Err(invalid("missing dictionary type"));
    };
    if fields.is_empty() && !object {
        return Ok(Type::Dictionary(plain));
    }
    if fields.len() > types.max_record_fields() {
        return Err(invalid("object field count exceeds limit"));
    }
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    if fields.windows(2).any(|pair| pair[0].name == pair[1].name) {
        return Err(unsupported("duplicate object key"));
    }
    for field in &fields {
        if field.optional && records::inherited(&field.name) {
            return Err(unsupported(
                "optional inherited property requires prototype semantics",
            ));
        }
        // TypeScript (TS2411): every property, including the `undefined` of an
        // optional one, must be assignable to the string index type.
        if !assignable(value, field.ty, types) {
            return Err(type_error(format!(
                "property {} of type {} is not assignable to the string index type {value}",
                field.name, field.ty
            )));
        }
        // V36 (`checked_declared_fields`) converts every declared type that
        // a JSON value graph can describe from an `unknown` index type.
        let convertible = types.profile.checked_declared_fields()
            && value == Type::Unknown
            && declared_fields::convertible(field.ty, types);
        if !convertible && !unknown::checkable(value, field.ty, types) {
            return Err(unsupported(&format!(
                "declared property {} of a mixed object type: its reads are checked against \
                 the index type {value}, and {} is not checkable from it",
                field.name, field.ty
            )));
        }
    }
    let members = MixedMembers { fields, object };
    let index = match types.mixed.iter().position(|m| m == &members) {
        Some(index) => index,
        None => {
            types.mixed.push(members);
            types.mixed.len() - 1
        }
    } as u32;
    let mixed = DictionaryType {
        declared: Some(index),
        ..types.dictionaries[plain as usize]
    };
    if let Some(id) = types.dictionaries.iter().position(|d| *d == mixed) {
        return Ok(Type::Dictionary(id as u32));
    }
    types.dictionaries.push(mixed);
    Ok(Type::Dictionary(types.dictionaries.len() as u32 - 1))
}

/// The static key of `d.name`, `d["name"]` or `d[0]`, if any.
pub(super) fn static_key(member: &ast::MemberExpr) -> Option<String> {
    match &member.prop {
        ast::MemberProp::Ident(id) => Some(id.sym.to_string()),
        ast::MemberProp::Computed(key)
            if matches!(
                &*key.expr,
                ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
            ) =>
        {
            records::expression_key(&key.expr).ok()
        }
        _ => None,
    }
}

/// The declared field named by a static member key of a mixed record.
pub(super) fn declared_field(
    ty: Type,
    member: &ast::MemberExpr,
    types: &ir::ValueTypes,
) -> Option<RecordField> {
    let name = static_key(member)?;
    types
        .declared(ty)?
        .fields
        .iter()
        .find(|field| field.name == name)
        .cloned()
}

/// Reject a dynamic or undeclared key on an `object` view: TypeScript types
/// it `any` (or rejects it), which Varta never admits.
pub(super) fn object_key(ty: Type, member: &ast::MemberExpr, types: &ir::ValueTypes) -> Result<()> {
    if !types.declared(ty).is_some_and(|members| members.object) {
        return Ok(());
    }
    Err(type_error(match static_key(member) {
        Some(name) => format!(
            "property {name} does not exist on an object narrowed from unknown; prove it with \
             `\"{name}\" in value` or assert the value `as Record<string, unknown>`"
        ),
        None => "dynamic key on an object narrowed from unknown would read any; assert the \
                 value `as Record<string, unknown>` first"
            .into(),
    }))
}

impl Lower<'_> {
    /// A declared read `d.name` of a mixed record: the stored entry converted
    /// to the declared type, `undefined` for an absent key whose type admits
    /// it, and a `TypeError` otherwise. The conversion is the checked test of
    /// an assertion unless the index type already is the declared type.
    pub(super) fn declared_read(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
        field: &RecordField,
    ) -> Result<Expr> {
        let key = Expr {
            span: member.prop.span(),
            ty: Type::String,
            kind: E::String(self.types.profile.string_bytes(&field.name)),
        };
        self.declared_get(member.span, value, key, field)
    }
    /// The declared read of `field` under the evaluated `key` (its name).
    pub(super) fn declared_get(
        &mut self,
        span: swc_common::Span,
        value: Expr,
        key: Expr,
        field: &RecordField,
    ) -> Result<Expr> {
        let wrong = format!("property {} does not hold its declared type", field.name);
        let missing = format!("declared property {} is missing", field.name);
        if let Type::Dictionary(id) = value.ty
            && declared_fields::applies(
                self.types.dictionaries[id as usize].value,
                field.ty,
                self.types,
            )
        {
            // V36: one shared read function per declared type converts the
            // entry and stores a built value back.
            return self.declared_call(span, value, key, field.ty, &wrong, &missing);
        }
        let (found, index, mut block) = self.dictionary_lookup(value, key)?;
        let output = self.hidden(field.ty);
        let converted = self.assert_value(found, field.ty, &field.name, &wrong, span)?;
        let absent = if admits_undefined(field.ty, self.types) {
            let undefined = Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            };
            S::Init(
                output,
                unions::widen(undefined, Some(field.ty), self.types)?,
            )
        } else {
            S::Throw(self.array_error(ErrorClass::TypeError, &missing, span)?)
        };
        block
            .body
            .push(S::If(index, vec![S::Init(output, converted)], vec![absent]));
        Ok(self.finish_dictionary(block, output, field.ty))
    }
}
