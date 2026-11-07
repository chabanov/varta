use super::*;
use std::collections::BTreeSet;
use varta_common::value::ErrorClass;

/// The `TypeError` message of a checked parse whose input does not match the
/// target type; the host raises it for schema mismatches and the guest for
/// the declared fields of a mixed record (v35).
const SCHEMA_MISMATCH: &str = "JSON value does not match the checked schema";
/// Why a checked decode target may not reach a `Uint8Array` under v38
/// (`byte_values`): JSON has no byte arrays, so a decoder would tag a JSON
/// number array (any numbers) as a `Uint8Array`.
const BYTES_TARGET: &str = "reaching a Uint8Array (JSON never produces a Uint8Array; decode a number[] and construct the Uint8Array from it)";

/// Whether checked parsing can build values of `ty` (`optional`: the position
/// may be missing, so `undefined` is admitted). Recursive types (v34) make the
/// type graph cyclic, so each type is visited once.
fn json_type(ty: Type, types: &ir::ValueTypes, optional: bool, nested: bool) -> bool {
    json_visit(ty, types, optional, nested, &mut BTreeSet::new(), false)
}
fn json_visit(
    ty: Type,
    types: &ir::ValueTypes,
    optional: bool,
    nested: bool,
    seen: &mut BTreeSet<(Type, bool, bool)>,
    constrained: bool,
) -> bool {
    // A type reached again nested is checked again: a mixed record is only
    // admitted at the root.
    if !seen.insert((ty, optional, nested)) {
        return true;
    }
    let mut child = |ty, optional| json_visit(ty, types, optional, true, seen, constrained);
    match ty {
        Type::Literal(..) => constrained,
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

/// Same-tag arrays or dictionary/record overlaps need strict matching, not
/// the decoder's kind dispatch. Plain dictionaries only; mixed fields have a
/// separate conversion contract and must not bypass its checks.
fn direct_kinds(ty: Type, types: &ir::ValueTypes, seen: &mut BTreeSet<Type>) -> bool {
    if !seen.insert(ty) {
        return true;
    }
    match ty {
        Type::Union(id) | Type::ReferenceUnion(id) => {
            let members = &types.unions[id as usize];
            let arrays = members
                .iter()
                .filter(|ty| matches!(ty, Type::Array(_)))
                .count();
            let records = members.iter().any(|ty| matches!(ty, Type::Record(_)));
            let dictionaries = members.iter().any(|ty| matches!(ty, Type::Dictionary(_)));
            arrays <= 1
                && !(records && dictionaries)
                && members.iter().all(|ty| direct_kinds(*ty, types, seen))
        }
        Type::Record(id) => types.records[id as usize]
            .fields
            .iter()
            .all(|field| direct_kinds(field.ty, types, seen)),
        Type::Array(id) => direct_kinds(types.arrays[id as usize], types, seen),
        Type::Dictionary(id) => {
            types.declared(ty).is_none()
                && direct_kinds(types.dictionaries[id as usize].value, types, seen)
        }
        _ => true,
    }
}

/// Keep only the semantic type edges needed by direct decoders. Erasure
/// still owns all physical types; these edges retain the literal constraints.
pub(super) fn preserve_direct_types(types: &mut ir::ValueTypes) {
    let mut pending: Vec<_> = types.direct_json_targets.iter().copied().collect();
    while let Some(ty) = pending.pop() {
        if types.direct_json_members.contains_key(&ty) {
            continue;
        }
        let children = match ty {
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .map(|f| f.ty)
                .collect(),
            Type::Array(id) => vec![types.arrays[id as usize]],
            Type::Dictionary(id) => vec![types.dictionaries[id as usize].value],
            Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize].clone(),
            _ => Vec::new(),
        };
        pending.extend(children.iter().copied());
        types.direct_json_members.insert(ty, children);
    }
}

/// Whether every value that parsing at the index type `value` builds for a
/// declared field of type `field` passes the field's check: primitive,
/// literal and nullish members, members of a closed index union (told apart by
/// their tag) and, under `unknown`, the dictionary of `unknown` that a JSON
/// object becomes.
fn parse_stable(field: Type, value: Type, types: &ir::ValueTypes) -> bool {
    // V36 converts the parsed JSON graph to any convertible declared type.
    if types.profile.checked_declared_fields()
        && value == Type::Unknown
        && declared_fields::convertible(field, types)
    {
        return true;
    }
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
    reaches_type(ty, Type::Unknown, types, seen)
}

/// Whether `ty` reaches `target` through unions, fields, elements or values.
fn reaches_type(ty: Type, target: Type, types: &ir::ValueTypes, seen: &mut BTreeSet<Type>) -> bool {
    if ty == target {
        return true;
    }
    if !seen.insert(ty) {
        return false;
    }
    match ty {
        Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
            .clone()
            .into_iter()
            .any(|member| reaches_type(member, target, types, seen)),
        Type::Record(id) => types.records[id as usize]
            .fields
            .clone()
            .into_iter()
            .any(|field| reaches_type(field.ty, target, types, seen)),
        Type::Array(id) => reaches_type(types.arrays[id as usize], target, types, seen),
        Type::Dictionary(id) => {
            reaches_type(types.dictionaries[id as usize].value, target, types, seen)
        }
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
        let name = self.quoted(target.span(), ty);
        self.refuse_byte_target("varta.json.parse", &name, ty, target.span())?;
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
        name: &str,
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
        {
            return Ok(None);
        }
        self.refuse_byte_target("JSON.parse(text) as", name, target, span)?;
        if literals::contains_pattern(target, self.types) {
            return Err(self.source.locate(
                unsupported(
                    "JSON.parse(text) as a type with a pattern template literal type; parse \
                     string and match the pattern explicitly",
                ),
                span,
            ));
        }
        if self.types.profile.json_decoders()
            && literals::contains(target, self.types)
            // Only the original checked graph conversion may be accelerated.
            // Views, tuples and nominal collections keep their existing path.
            && declared_fields::convertible(target, self.types)
            && matches!(
                target,
                Type::Record(_) | Type::ReferenceUnion(_) | Type::Array(_)
            )
            && json_visit(target, self.types, false, true, &mut BTreeSet::new(), true)
            && !self.undiscriminated_unions(target)
            && direct_kinds(target, self.types, &mut BTreeSet::new())
        {
            return self
                .direct_json_assertion(&call.args[0].expr, target, name, span, call.span)
                .map(Some);
        }
        if literals::contains(target, self.types)
            || !json_type(target, self.types, false, false)
            || self.undiscriminated_unions(target)
        {
            return Ok(None);
        }
        self.parse_json_at(&call.args[0].expr, target, span)
            .map(Some)
    }
    /// A compiler-private attempt with an allocated-reference result. Zero is
    /// an internal miss, never a language value: identity inspects only its
    /// address bits, and the normal conversion completes it before publication.
    fn direct_json_assertion(
        &mut self,
        argument: &ast::Expr,
        target: Type,
        name: &str,
        span: swc_common::Span,
        call_span: swc_common::Span,
    ) -> Result<Expr> {
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
        unknown_graph(self.types)?;
        let input = self.hidden(Type::String);
        let output = self.hidden(target);
        let local = |slot, ty| Expr {
            span,
            ty,
            kind: E::Local(slot),
        };
        let ordinary = Expr {
            span: call_span,
            ty: Type::Unknown,
            kind: E::ParseJson(Box::new(local(input, Type::String))),
        };
        let message = format!("type assertion to {name} failed");
        // Build the real conversion before registering the optimization, so
        // unsupported assertion targets keep their established diagnostics.
        let fallback = self.assert_value(ordinary, target, name, &message, span)?;
        self.types.direct_json_targets.insert(target);
        let attempt = Expr {
            span,
            ty: target,
            kind: E::ParseJson(Box::new(local(input, Type::String))),
        };
        let address = Expr {
            span,
            ty: Type::Float64,
            kind: E::Intrinsic(ir::Intrinsic::Identity, vec![local(output, target)]),
        };
        let miss = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Binary(
                ast::BinaryOp::EqEqEq,
                Box::new(address),
                Box::new(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Float64(ir::JSON_DECODE_MISS as f64),
                }),
            ),
        };
        Ok(Expr {
            span,
            ty: target,
            kind: E::ValueBlock {
                output,
                marker: self.hidden(Type::Number),
                roots: vec![input, output],
                body: vec![
                    S::Init(input, text),
                    S::Init(output, attempt),
                    S::If(miss, vec![S::Set(output, fallback)], vec![]),
                ],
            },
        })
    }
    /// A KV key or value decode at `ty` (v38 `kv`): the targets, schema and
    /// declared-field checks of checked parsing, over the stored entry
    /// `index` of the last read instead of JSON text. No target reaching a
    /// `Uint8Array` gets here: keys decode at the prelude's fixed `KvKey`,
    /// and a value target is either a top-level `Uint8Array` (read raw by
    /// `bytesAt`) or passed `kv_checked`, which refuses nested ones.
    pub(super) fn kv_decode(
        &mut self,
        host: Host,
        index: Expr,
        ty: Type,
        span: swc_common::Span,
    ) -> Result<Expr> {
        if literals::contains(ty, self.types) {
            return Err(self.source.locate(
                unsupported(
                    "KV value type with a literal type; read the base primitive and narrow it",
                ),
                span,
            ));
        }
        if !json_type(ty, self.types, false, false)
            || reaches_type(ty, Type::BigInt, self.types, &mut BTreeSet::new())
        {
            return Err(self.source.locate(
                unsupported(&format!(
                    "KV value type {ty}; KV values are checked JSON types without BigInt"
                )),
                span,
            ));
        }
        if reaches_unknown(ty, self.types, &mut BTreeSet::new()) {
            unknown_graph(self.types)?;
        }
        let decoded = Expr {
            span,
            ty,
            kind: E::KvDecode(host, Box::new(index)),
        };
        match self.types.declared(ty).cloned() {
            Some(members) => self.validate_declared(decoded, &members.fields, span),
            None => Ok(decoded),
        }
    }
    /// Refuse a checked decode target `ty` (`name`, decoded by `what`) that
    /// reaches a `Uint8Array` under v38 (`byte_values`), where a
    /// `Uint8Array` carries its own value tag. Earlier profiles keep their
    /// targets.
    fn refuse_byte_target(
        &self,
        what: &str,
        name: &str,
        ty: Type,
        span: swc_common::Span,
    ) -> Result<()> {
        if self.types.profile.byte_values() && collections::reaches_bytes(ty, self.types) {
            return Err(self.source.locate(
                unsupported(&format!(
                    "checked JSON decode target: {what} {name} {BYTES_TARGET}"
                )),
                span,
            ));
        }
        Ok(())
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
        let Type::Dictionary(dictionary) = ty else {
            return Err(invalid("missing dictionary type"));
        };
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
            if declared_fields::applies(
                self.types.dictionaries[dictionary as usize].value,
                field.ty,
                self.types,
            ) {
                // V36: convert the entry and store the result back, so the
                // parsed value already holds the declared type.
                let read = self.declared_call(
                    span,
                    receiver,
                    key,
                    field.ty,
                    SCHEMA_MISMATCH,
                    SCHEMA_MISMATCH,
                )?;
                body.push(S::Expr(read));
                continue;
            }
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
