//! Prepared String-key assignments with exact, selected-field type checks.
use super::*;

const INVALID_FIELD_VALUE: &str = "Invalid computed record field value";
const UNKNOWN_WRITE_KEY: &str = "Unknown computed record write key";

struct PreparedWrite {
    span: swc_common::Span,
    source: u32,
    input: u32,
    record: Type,
    value: Type,
}
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn literal_context(
    rhs: &ast::Expr,
    fields: &[ir::RecordField],
    types: &ir::ValueTypes,
) -> Option<Type> {
    // Contextualize only an unambiguous aggregate literal. Combining all fields
    // would invent an erased type or reject otherwise valid independent shapes.
    let mut candidates: Vec<_> = fields
        .iter()
        .flat_map(|field| unions::members(field.ty, types))
        .filter(|ty| match stmt::unparen(rhs) {
            ast::Expr::Object(_) => matches!(ty, Type::Record(_)),
            ast::Expr::Array(_) => matches!(ty, Type::Array(_)),
            _ => false,
        })
        .collect();
    candidates.sort();
    candidates.dedup();
    if candidates.len() == 1 {
        candidates.first().copied()
    } else {
        None
    }
}
fn member_test(write: &PreparedWrite, allowed: Vec<Type>) -> Expr {
    let span = write.span;
    allowed
        .into_iter()
        .rev()
        .fold(boolean(span, false), |rest, member| Expr {
            span,
            ty: Type::Boolean,
            kind: E::Conditional(
                Box::new(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUnionMember(Box::new(local(span, write.input, write.value)), member),
                }),
                Box::new(boolean(span, true)),
                Box::new(rest),
            ),
        })
}
impl Lower<'_> {
    pub(super) fn dynamic_record_write_value(
        &mut self,
        assign: &ast::AssignExpr,
        base: Expr,
        key: &ast::Expr,
        shape: u32,
    ) -> Result<Expr> {
        let key = self.expr(key)?;
        if key.ty != Type::String {
            return Err(self.source.locate(
                type_error(
                    "computed record key must be a narrowed string; coercion is unsupported",
                ),
                key.span,
            ));
        }
        let value = self.record_assignment_rhs(&assign.right, shape)?;
        self.checked_record_write_value(assign.span, base, key, value, shape)
    }
    pub(super) fn record_assignment_rhs(&mut self, rhs: &ast::Expr, shape: u32) -> Result<Expr> {
        let fields = &self.types.records[shape as usize].fields;
        let context = literal_context(rhs, fields, self.types);
        self.expr_expected(rhs, context)
    }
    pub(super) fn checked_record_write_value(
        &mut self,
        span: swc_common::Span,
        base: Expr,
        key: Expr,
        value: Expr,
        shape: u32,
    ) -> Result<Expr> {
        let fields = self.types.records[shape as usize].fields.clone();
        let actual = unions::members(value.ty, self.types);
        if actual.iter().any(|ty| ty.value_kind().is_none()) {
            return Err(self
                .source
                .locate(unsupported("computed record write value type"), value.span));
        }
        if !fields.is_empty()
            && !fields.iter().any(|field| {
                let expected = unions::members(field.ty, self.types);
                actual.iter().any(|ty| expected.contains(ty))
            })
        {
            return Err(self.source.locate(
                type_error("computed record write value matches no declared field type"),
                value.span,
            ));
        }
        let write = PreparedWrite {
            span,
            source: self.hidden(Type::Record(shape)),
            input: self.hidden(value.ty),
            record: Type::Record(shape),
            value: value.ty,
        };
        let name = self.hidden(Type::String);
        let matched = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let mut entries = Vec::new();
        for (slot, field) in fields.iter().enumerate() {
            let mut body = self.record_write_leaf(&write, slot as u32, field.ty)?;
            body.push(S::Set(matched, boolean(span, true)));
            entries.push((field.name.as_str(), body));
        }
        entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
        let mut entries: Vec<_> = entries
            .into_iter()
            .map(|(name, body)| (self.types.profile.string_bytes(name), body))
            .collect();
        let mut body = vec![
            S::Init(write.source, base),
            S::Init(name, key),
            S::Init(write.input, value),
            S::Init(matched, boolean(span, false)),
        ];
        body.extend(own_keys::lookup(span, name, &mut entries));
        body.push(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Not(Box::new(local(span, matched, Type::Boolean))),
            },
            vec![S::Throw(self.record_write_error(UNKNOWN_WRITE_KEY, span)?)],
            vec![],
        ));
        Ok(Expr {
            span,
            ty: write.value,
            kind: E::ValueBlock {
                output: write.input,
                marker,
                roots: vec![write.source, name, write.input, matched],
                body,
            },
        })
    }
    pub(super) fn assign_record_field(
        &mut self,
        span: swc_common::Span,
        source: u32,
        shape: u32,
        name: &str,
        value: Expr,
    ) -> Result<S> {
        let fields = &self.types.records[shape as usize].fields;
        let (slot, field) = fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == name)
            .ok_or_else(|| {
                unsupported("Object.assign cannot add undeclared checked record fields")
            })?;
        let expected = field.ty;
        let write = PreparedWrite {
            span,
            source,
            input: self.hidden(value.ty),
            record: Type::Record(shape),
            value: value.ty,
        };
        let mut body = vec![S::Init(write.input, value)];
        body.extend(self.record_write_leaf(&write, slot as u32, expected)?);
        Ok(S::Scope(vec![write.input], body))
    }
    fn record_write_leaf(
        &mut self,
        write: &PreparedWrite,
        slot: u32,
        expected: Type,
    ) -> Result<Vec<S>> {
        let actual = unions::members(write.value, self.types);
        let expected_members = unions::members(expected, self.types);
        let allowed: Vec<_> = actual
            .iter()
            .copied()
            .filter(|ty| expected_members.contains(ty))
            .collect();
        if allowed.is_empty() {
            return Ok(vec![S::Throw(
                self.record_write_error(INVALID_FIELD_VALUE, write.span)?,
            )]);
        }
        // Match full static types first. A runtime record/array tag is enough
        // only because each input union has one proven shape/element type.
        let narrowed = unions::intern(allowed.clone(), self.types)?;
        let payload = if write.value.is_union() && !narrowed.is_union() {
            Expr {
                span: write.span,
                ty: narrowed,
                kind: E::UnboxUnion(Box::new(local(write.span, write.input, write.value))),
            }
        } else {
            local(write.span, write.input, narrowed)
        };
        let store = S::SetField(
            local(write.span, write.source, write.record),
            slot,
            unions::widen(payload, Some(expected), self.types)?,
        );
        if allowed.len() == actual.len() {
            return Ok(vec![store]);
        }
        Ok(vec![S::If(
            member_test(write, allowed),
            vec![store],
            vec![S::Throw(
                self.record_write_error(INVALID_FIELD_VALUE, write.span)?,
            )],
        )])
    }
    fn record_write_error(&mut self, message: &str, span: swc_common::Span) -> Result<Expr> {
        self.array_error(varta_common::value::ErrorClass::TypeError, message, span)
    }
}
