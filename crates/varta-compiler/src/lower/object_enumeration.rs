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
        // V38 (`declaration_forms`): a class object's own static fields.
        if let [Type::ClassValue(_)] = present.as_slice()
            && matches!(value.ty, Type::ClassValue(_))
        {
            return match kind {
                Kind::Keys => self.class_value_keys(value, false, span),
                // tsc types it `any[]` (`typeof C` has no index signature).
                Kind::Values => Err(self.source.locate(
                    type_error("Object.values of a class object is typed any[] (untyped code)"),
                    span,
                )),
            };
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
        // V38 (`sdk_paths`): a view of wider objects is refused by
        // `subtyping::resolve` (its keys are the view's, not the object's).
        // A synthesized layout copy reads one exact layout (`layout_spreads`);
        // keys enumerate the object's own layout (`subtyping` dispatches the
        // enumeration on its descriptor), so only values are noted.
        if self.types.profile.sdk_paths()
            && self.types.records[shape as usize].class.is_none()
            && !self.types.layouts.exact
            && !matches!(kind, Kind::Keys)
            && !self.types.profile.expression_forms()
        {
            self.types.subtyping.note_key_copy(
                shape,
                subtyping::KeyCopy::Enumeration,
                self.source,
                span,
            );
        }
        // V38: values of another layout's undeclared keys are checked
        // (`subtyping::checked_value`).
        if self.types.profile.expression_forms() && matches!(kind, Kind::Values) {
            subtyping::note_values(shape, self.types).map_err(|e| self.source.locate(e, span))?;
        }
        let fields = self.types.records[shape as usize].fields.clone();
        // V38 (`symbol_values`): a symbol-keyed member is no string key, and
        // the enumeration compacts by canonical slot; such records are
        // refused rather than listed with their symbol keys.
        if fields
            .iter()
            .any(|field| crate::modules::symbol_keys::is_symbol_key(&field.name))
        {
            return Err(self.source.locate(
                unsupported(&format!(
                    "{name} of an object with symbol-keyed members (string-key enumeration that skips them is not implemented)"
                )),
                value.span,
            ));
        }
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
            let values = self.enumeration_values(kind, shape, source, element, span)?;
            if self.types.receivers.has_accessors() {
                // V38 (`object_accessors`): a getter runs when its key is
                // reached, in the object's own-key order.
                body.extend(self.values_in_key_order(
                    values,
                    (source, record),
                    element,
                    span,
                    &mut roots,
                    &mut slots,
                )?);
            } else {
                for value in values {
                    let slot = self.hidden(element);
                    roots.push(slot);
                    body.push(S::Init(slot, value));
                    slots.push(slot);
                }
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
    /// Read `values` (one per field of the record) into new rooted slots in
    /// the order of the object's own keys: the keys are enumerated first,
    /// then each key's value is read when it is reached.
    fn values_in_key_order(
        &mut self,
        values: Vec<Expr>,
        (source, record): (u32, Type),
        element: Type,
        span: swc_common::Span,
        roots: &mut Vec<u32>,
        slots: &mut Vec<u32>,
    ) -> Result<Vec<S>> {
        let Type::Record(shape) = record else {
            return Err(invalid("missing enumeration record"));
        };
        let names: Vec<String> = self.types.records[shape as usize]
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect();
        let mut body = Vec::new();
        for _ in &values {
            let slot = self.hidden(element);
            roots.push(slot);
            body.push(S::Init(
                slot,
                Expr {
                    span,
                    ty: element,
                    kind: E::Zero,
                },
            ));
            slots.push(slot);
        }
        let keys = self.enumerate_value(local(span, source, record), Kind::Keys, span)?;
        let keys_ty = keys.ty;
        let key_slot = self.hidden(keys_ty);
        let index = self.hidden(Type::Float64);
        roots.push(key_slot);
        body.push(S::Init(key_slot, keys));
        body.push(S::Init(
            index,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Float64(0.0),
            },
        ));
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::Index(
                Box::new(local(span, key_slot, keys_ty)),
                Box::new(local(span, index, Type::Float64)),
            ),
        };
        let mut dispatch = Vec::new();
        for ((name, value), slot) in names.iter().zip(values).zip(slots.iter()).rev() {
            let test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::EqEqEq,
                    Box::new(key.clone()),
                    Box::new(Expr {
                        span,
                        ty: Type::String,
                        kind: E::String(self.types.profile.string_bytes(name)),
                    }),
                ),
            };
            dispatch = vec![S::If(test, vec![S::Set(*slot, value)], dispatch)];
        }
        dispatch.push(S::Set(
            index,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Binary(
                    ast::BinaryOp::Add,
                    Box::new(local(span, index, Type::Float64)),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Float64(1.0),
                    }),
                ),
            },
        ));
        body.push(S::While(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::Lt,
                    Box::new(local(span, index, Type::Float64)),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Length(Box::new(local(span, key_slot, keys_ty))),
                    }),
                ),
            },
            dispatch,
        ));
        Ok(body)
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
