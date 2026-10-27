//! Typed computed String reads without coercion or prototype traversal.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

impl Lower<'_> {
    pub(super) fn dynamic_record_read(
        &mut self,
        span: swc_common::Span,
        value: Expr,
        key: Expr,
        shape: u32,
    ) -> Result<Expr> {
        let fields = self.types.records[shape as usize].fields.clone();
        let result = unions::intern(
            fields
                .iter()
                .map(|field| field.ty)
                .chain([Type::Undefined])
                .collect(),
            self.types,
        )
        .map_err(|error| self.source.locate(error, span))?;
        let record = Type::Record(shape);
        let source = self.hidden(record);
        let name = self.hidden(Type::String);
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        let mut entries = Vec::new();
        for (slot, field) in fields.iter().enumerate() {
            let payload = unions::widen(
                Expr {
                    span,
                    ty: field.ty,
                    kind: E::Field(Box::new(local(span, source, record)), slot as u32),
                },
                Some(result),
                self.types,
            )?;
            entries.push((
                field.name.as_str(),
                vec![S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::HasOwn(Box::new(local(span, source, record)), Some(slot as u32)),
                    },
                    vec![S::Set(output, payload)],
                    vec![],
                )],
            ));
        }
        // The static path rejects inherited names. A dynamic name cannot be
        // rejected at compile time: fail explicitly rather than inventing an
        // Undefined value for a property that needs prototype semantics.
        for inherited in records::INHERITED_NAMES {
            if fields.iter().any(|field| field.name == *inherited) {
                continue;
            }
            let error = self.array_error(
                varta_common::value::ErrorClass::TypeError,
                "Inherited record property access is unsupported",
                span,
            )?;
            entries.push((inherited, vec![S::Throw(error)]));
        }
        entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
        let mut entries: Vec<_> = entries
            .into_iter()
            .map(|(name, body)| (self.types.profile.string_bytes(name), body))
            .collect();
        let missing = unions::widen(
            Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            },
            Some(result),
            self.types,
        )?;
        let mut body = vec![
            S::Init(source, value),
            S::Init(name, key),
            S::Init(output, missing),
        ];
        body.extend(own_keys::lookup(span, name, &mut entries));
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![source, name, output],
                body,
            },
        })
    }
}
