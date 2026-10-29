use super::*;
use std::collections::BTreeSet;
use varta_common::value::ErrorClass;

/// The `TypeError` message of a checked parse whose input does not match the
/// target type; the host raises it for schema mismatches and the guest for
/// the declared fields of a mixed record (v35).
const SCHEMA_MISMATCH: &str = "JSON value does not match the checked schema";

/// Whether checked parsing can build values of `ty` (`optional`: the position
/// may be missing, so `undefined` is admitted). Recursive types (v34) make the
/// type graph cyclic, so each type is visited once.
fn json_type(ty: Type, types: &ir::ValueTypes, optional: bool, nested: bool) -> bool {
    json_visit(ty, types, optional, nested, &mut BTreeSet::new())
}
fn json_visit(
    ty: Type,
    types: &ir::ValueTypes,
    optional: bool,
    nested: bool,
    seen: &mut BTreeSet<(Type, bool, bool)>,
) -> bool {
    // A type reached again nested is checked again: a mixed record is only
    // admitted at the root.
    if !seen.insert((ty, optional, nested)) {
        return true;
    }
    let mut child = |ty, optional| json_visit(ty, types, optional, true, seen);
    match ty {
        Type::Literal(..) => false,
        // `unknown` (v35 `unknown_json`) parses any JSON value into the
        // internal JSON value graph; v34 has no schema for it.
        Type::Unknown => types.profile.unknown_json(),
        Type::BigInt => types.profile.bigint_values(),
        // Parsing cannot construct nominal class instances.
        Type::Error | Type::NativeError(_) | Type::Class(_) => false,
        Type::Undefined => types.profile.checked_json() && optional,
        Type::Union(id) | Type::ReferenceUnion(id) => {
            types.profile.checked_json()
                && types.unions[id as usize]
                    .iter()
                    .all(|member| child(*member, true))
        }
        Type::Record(id) => types.records[id as usize]
            .fields
            .iter()
            .all(|field| child(field.ty, field.optional)),
        Type::Array(id) => child(types.arrays[id as usize], false),
        Type::Dictionary(id) => {
            // The declared fields of a mixed record are validated after
            // parsing, which the root alone does.
            let declared = types.declared(ty).is_none_or(|members| {
                !nested
                    && !members.object
                    && members.fields.iter().all(|field| {
                        parse_stable(field.ty, types.dictionaries[id as usize].value, types)
                    })
            });
            declared && child(types.dictionaries[id as usize].value, true)
        }
        _ => ty.value_kind().is_some(),
    }
}

/// Whether every value that parsing at the index type `value` builds for a
/// declared field of type `field` passes the field's check: primitive,
/// literal and nullish members, members of a closed index union (told apart by
/// their tag) and, under `unknown`, the dictionary of `unknown` that a JSON
/// object becomes.
fn parse_stable(field: Type, value: Type, types: &ir::ValueTypes) -> bool {
    let members = unions::members(value, types);
    unions::members(field, types).into_iter().all(|member| {
        member == value
            || unions::primitive(member)
            || member == Type::BigInt
            || members.contains(&member)
            || value == Type::Unknown
                && matches!(member, Type::Dictionary(id)
                    if types.dictionaries[id as usize].value == Type::Unknown
                        && types.declared(member).is_none())
    })
}

/// Whether `ty` reaches `unknown`.
fn reaches_unknown(ty: Type, types: &ir::ValueTypes, seen: &mut BTreeSet<Type>) -> bool {
    if ty == Type::Unknown {
        return true;
    }
    if !seen.insert(ty) {
        return false;
    }
    match ty {
        Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
            .clone()
            .into_iter()
            .any(|member| reaches_unknown(member, types, seen)),
        Type::Record(id) => types.records[id as usize]
            .fields
            .clone()
            .into_iter()
            .any(|field| reaches_unknown(field.ty, types, seen)),
        Type::Array(id) => reaches_unknown(types.arrays[id as usize], types, seen),
        Type::Dictionary(id) => reaches_unknown(types.dictionaries[id as usize].value, types, seen),
        _ => false,
    }
}

/// The array and dictionary types of the JSON value graph that parsing at
/// `unknown` builds (v35 `unknown_json`): `unknown[]` for JSON arrays and
/// `Record<string, unknown>` for JSON objects.
pub(super) fn unknown_graph(types: &mut ir::ValueTypes) -> Result<(Type, Type)> {
    Ok((
        arrays::intern(Type::Unknown, types)?,
        dictionaries::intern(Type::Unknown, types)?,
    ))
}

/// The interned graph types of [`unknown_graph`], for schema emission.
pub(crate) fn unknown_graph_types(types: &ir::ValueTypes) -> Option<(u32, u32)> {
    let array = types.arrays.iter().position(|ty| *ty == Type::Unknown)?;
    let dictionary = types
        .dictionaries
        .iter()
        .position(|d| d.value == Type::Unknown && d.declared.is_none())?;
    Some((array as u32, dictionary as u32))
}

impl Lower<'_> {
    pub(super) fn parse_json(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if call.args.len() != 1 || call.args[0].spread.is_some() {
            return Err(invalid("varta.json.parse<T> expects one string argument"));
        }
        let arguments = call
            .type_args
            .as_ref()
            .filter(|a| a.params.len() == 1)
            .ok_or_else(|| invalid("varta.json.parse requires exactly one checked value type"))?;
        let target = &arguments.params[0];
        let ty =
            keyword_type(target, self.types).map_err(|e| self.source.locate(e, target.span()))?;
        if literals::contains(ty, self.types) {
            // The schema ABI cannot express value sets; unchecked members would
            // otherwise reach literal-typed places.
            return Err(self.source.locate(
                unsupported("JSON parsing target with a literal type; parse the base primitive and narrow it"),
                target.span(),
            ));
        }
        if !json_type(ty, self.types, false, false) {
            return Err(self
                .source
                .locate(unsupported("JSON parsing target type"), target.span()));
        }
        self.parse_json_at(&call.args[0].expr, ty, call.span)
    }
    /// `JSON.parse(text)` (v35 `unknown_json`): the standard global returns
    /// `unknown` (TypeScript's `any` is never admitted), a graph of
    /// primitives, `unknown[]` arrays and `Record<string, unknown>` objects.
    pub(super) fn global_parse_json(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if call.args.len() != 1 || call.args[0].spread.is_some() {
            return Err(unsupported(
                "JSON.parse with a reviver or other than one string argument",
            ));
        }
        self.parse_json_at(&call.args[0].expr, Type::Unknown, call.span)
    }
    /// `JSON.parse(text) as T` (v35 `unknown_json`): when `T` is a checked
    /// parse target, the assertion is the checked parse at `T`, which proves
    /// the whole graph instead of testing only the root of an `unknown` graph.
    /// `None` when the operand is not such a call or `T` is no parse target.
    pub(super) fn asserted_parse_json(
        &mut self,
        operand: &ast::Expr,
        target: Type,
        span: swc_common::Span,
    ) -> Result<Option<Expr>> {
        let ast::Expr::Call(call) = operand else {
            return Ok(None);
        };
        let ast::Callee::Expr(callee) = &call.callee else {
            return Ok(None);
        };
        if !self.types.profile.unknown_json()
            || expr::callee_name(callee).as_deref() != Some("JSON.parse")
            || call.args.len() != 1
            || call.args[0].spread.is_some()
            || literals::contains(target, self.types)
            || !json_type(target, self.types, false, false)
        {
            return Ok(None);
        }
        self.parse_json_at(&call.args[0].expr, target, span)
            .map(Some)
    }
    fn parse_json_at(
        &mut self,
        argument: &ast::Expr,
        ty: Type,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if reaches_unknown(ty, self.types, &mut BTreeSet::new()) {
            unknown_graph(self.types)?;
        }
        let text = self.expr(argument)?;
        if text.ty != Type::String {
            return Err(self.source.locate(
                type_error(format!(
                    "JSON input type mismatch: expected string, found {}",
                    text.ty
                )),
                text.span,
            ));
        }
        let parsed = Expr {
            span,
            ty,
            kind: E::ParseJson(Box::new(text)),
        };
        match self.types.declared(ty).cloned() {
            Some(members) => self.validate_declared(parsed, &members.fields, span),
            None => Ok(parsed),
        }
    }
    /// Check the declared fields of a parsed mixed record (v35): a missing
    /// required field or a field of another type throws the checked-schema
    /// `TypeError`, exactly as a schema mismatch does.
    fn validate_declared(
        &mut self,
        parsed: Expr,
        fields: &[ir::RecordField],
        span: swc_common::Span,
    ) -> Result<Expr> {
        let ty = parsed.ty;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let mut body = vec![S::Init(output, parsed)];
        let roots = vec![output];
        for field in fields {
            let key = Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(&field.name)),
            };
            let receiver = Expr {
                span,
                ty,
                kind: E::Local(output),
            };
            let (found, index, mut block) = self.dictionary_lookup(receiver, key)?;
            let converted =
                self.assert_value(found, field.ty, &field.name, SCHEMA_MISMATCH, span)?;
            let absent = if field.optional {
                vec![]
            } else {
                vec![S::Throw(self.array_error(
                    ErrorClass::TypeError,
                    SCHEMA_MISMATCH,
                    span,
                )?)]
            };
            block
                .body
                .push(S::If(index, vec![S::Expr(converted)], absent));
            let done = self.hidden(Type::Boolean);
            block.body.push(S::Init(
                done,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(1),
                },
            ));
            body.push(S::Expr(self.finish_dictionary(block, done, Type::Boolean)));
        }
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
