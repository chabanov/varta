//! Checked own-property enumeration with invariant homogeneous array elements.
use super::*;
#[derive(Clone, Copy)]
pub(super) enum Kind {
    Keys,
    Values,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Keys => "Object.keys",
            Self::Values => "Object.values",
        }
    }
}
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
impl Lower<'_> {
    pub(super) fn object_enumeration(&mut self, call: &ast::CallExpr, kind: Kind) -> Result<Expr> {
        let name = kind.name();
        if !self.types.profile.closures() {
            return Err(unsupported(&format!(
                "{name} requires checked-f64-utf16-v12"
            )));
        }
        if self.binding("Object").is_ok() || self.functions.contains_key("Object") {
            return Err(unsupported(&format!("shadowed {name} intrinsic")));
        }
        if self.types.profile.property_deletion() {
            if call.type_args.is_some() {
                return Err(invalid(format!("{name} has no type arguments")));
            }
            let span = call.span;
            let value = if let Some(first) = call.args.first() {
                self.object_value(&first.expr)?
            } else {
                Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                }
            };
            let ty = value.ty;
            let input = self.hidden(ty);
            let mut body = vec![S::Init(input, value)];
            for (index, arg) in call.args.iter().enumerate() {
                if arg.spread.is_some() {
                    return Err(unsupported(&format!("{name} argument spread")));
                }
                if index > 0 {
                    body.push(S::Expr(self.expr(&arg.expr)?));
                }
            }
            let result = self.enumerate_value(local(span, input, ty), kind, span)?;
            let output = self.hidden(result.ty);
            let marker = self.hidden(Type::Number);
            let result_ty = result.ty;
            body.push(S::Init(output, result));
            return Ok(Expr {
                span,
                ty: result_ty,
                kind: E::ValueBlock {
                    output,
                    marker,
                    roots: vec![input, output],
                    body,
                },
            });
        }
        if call.args.len() != 1 || call.type_args.is_some() {
            return Err(invalid(format!(
                "{name} expects one value and no type arguments"
            )));
        }
        let value = self.expr(&call.args[0].expr)?;
        self.enumerate_value(value, kind, call.span)
    }
    pub(super) fn enumerate_value(
        &mut self,
        value: Expr,
        kind: Kind,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let name = kind.name();
        let present: Vec<_> = unions::members(value.ty, self.types)
            .into_iter()
            .filter(|ty| !matches!(ty, Type::Null | Type::Undefined))
            .collect();
        if self.types.profile.property_deletion() {
            if present == [Type::String] {
                return self.string_enumeration(value, kind, span);
            }
            if present.contains(&Type::String)
                && present.iter().all(|ty| {
                    matches!(
                        ty,
                        Type::String | Type::Float64 | Type::Boolean | Type::BigInt
                    )
                })
            {
                return self.mixed_primitive_enumeration(value, kind, span);
            }
            if present
                .iter()
                .all(|ty| matches!(ty, Type::Float64 | Type::Boolean | Type::BigInt))
            {
                return self.empty_primitive_enumeration(value, kind, span);
            }
        }
        if let [Type::Array(id)] = present.as_slice() {
            return self.array_enumeration(value, *id, kind, span);
        }
        if let [Type::Dictionary(_)] = present.as_slice()
            && matches!(value.ty, Type::Dictionary(_))
        {
            return self.dictionary_enumeration(value, matches!(kind, Kind::Keys));
        }
        let shape = match present.as_slice() {
            [Type::Record(id)] => *id,
            _ => {
                return Err(self.source.locate(
                    type_error(format!("{name} requires a closed record or checked array")),
                    value.span,
                ));
            }
        };
        let fields = self.types.records[shape as usize].fields.clone();
        let element = match kind {
            Kind::Keys => Type::String,
            Kind::Values if fields.is_empty() => Type::Undefined,
            Kind::Values => {
                unions::intern(fields.iter().map(|field| field.ty).collect(), self.types)
                    .map_err(|e| self.source.locate(e, span))?
            }
        };
        let record = Type::Record(shape);
        let (source, mut roots, mut body) = self.enumeration_source(value, record, span)?;
        let result = arrays::intern(element, self.types)?;
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        roots.push(output);
        // Prepare values once in rooted slots. Compaction then reads only
        // literals or locals and cannot allocate while rearranging the array.
        let cached = if matches!(kind, Kind::Values) {
            let mut slots = Vec::new();
            for value in self.enumeration_values(kind, shape, source, element, span)? {
                let slot = self.hidden(element);
                roots.push(slot);
                body.push(S::Init(slot, value));
                slots.push(slot);
            }
            Some(slots)
        } else {
            None
        };
        let entries = |this: &Self| -> Result<Vec<Expr>> {
            if let Some(slots) = &cached {
                Ok(slots
                    .iter()
                    .map(|slot| local(span, *slot, element))
                    .collect())
            } else {
                this.enumeration_values(kind, shape, source, element, span)
            }
        };
        let elements = entries(self)?.into_iter().map(Some).collect();
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: result,
                kind: E::Array(elements),
            },
        ));
        let values = entries(self)?;
        body.push(S::RecordEnumerate {
            source,
            target: output,
            shape,
            slots: [
                self.hidden(Type::Number),
                self.hidden(Type::Number),
                self.hidden(Type::Number),
                self.hidden(Type::Number),
            ],
            values,
        });
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
    fn array_enumeration(
        &mut self,
        value: Expr,
        id: u32,
        kind: Kind,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let array = Type::Array(id);
        let element = match kind {
            Kind::Keys => Type::String,
            Kind::Values => self.types.arrays[id as usize],
        };
        let result = arrays::intern(element, self.types)?;
        let (source, mut roots, mut body) = self.enumeration_source(value, array, span)?;
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        roots.push(output);
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: result,
                kind: E::ArrayEnumerate {
                    value: Box::new(local(span, source, array)),
                    keys: matches!(kind, Kind::Keys),
                },
            },
        ));
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
    pub(super) fn enumeration_source(
        &mut self,
        value: Expr,
        narrowed: Type,
        span: swc_common::Span,
    ) -> Result<(u32, Vec<u32>, Vec<S>)> {
        let input_ty = value.ty;
        let input = self.hidden(input_ty);
        let mut roots = vec![input];
        let mut body = vec![S::Init(input, value)];
        if input_ty == narrowed {
            return Ok((input, roots, body));
        }
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Cannot convert undefined or null to object",
            span,
        )?;
        body.push(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsNullish(Box::new(local(span, input, input_ty))),
            },
            vec![S::Throw(error)],
            vec![],
        ));
        let source = self.hidden(narrowed);
        roots.push(source);
        body.push(S::Init(
            source,
            Expr {
                span,
                ty: narrowed,
                kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
            },
        ));
        Ok((source, roots, body))
    }
    fn enumeration_values(
        &self,
        kind: Kind,
        shape: u32,
        source: u32,
        element: Type,
        span: swc_common::Span,
    ) -> Result<Vec<Expr>> {
        self.types.records[shape as usize]
            .fields
            .iter()
            .enumerate()
            .map(|(index, field)| match kind {
                Kind::Keys => Ok(Expr {
                    span,
                    ty: Type::String,
                    kind: E::String(self.types.profile.string_bytes(&field.name)),
                }),
                Kind::Values => unions::widen(
                    Expr {
                        span,
                        ty: field.ty,
                        kind: E::Field(
                            Box::new(local(span, source, Type::Record(shape))),
                            index as u32,
                        ),
                    },
                    Some(element),
                    self.types,
                ),
            })
            .collect()
    }
}
