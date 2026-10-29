//! Ordered own-property copies into existing checked record/array storage.
use super::object_enumeration::Kind;
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, n: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(n),
    }
}
fn selected(span: swc_common::Span, slot: u32, source: Type, ty: Type) -> Expr {
    if source == ty {
        local(span, slot, ty)
    } else {
        Expr {
            span,
            ty,
            kind: E::UnboxUnion(Box::new(local(span, slot, source))),
        }
    }
}
fn index(span: swc_common::Span, array: u32, ty: Type, at: u32, element: Type) -> Expr {
    Expr {
        span,
        ty: element,
        kind: E::Index(
            Box::new(local(span, array, ty)),
            Box::new(local(span, at, Type::Float64)),
        ),
    }
}
fn copy_loop(span: swc_common::Span, at: u32, length: Expr, body: Vec<S>) -> Vec<S> {
    let mut body = body;
    body.push(S::Set(
        at,
        Expr {
            span,
            ty: Type::Float64,
            kind: E::Binary(
                ast::BinaryOp::Add,
                Box::new(local(span, at, Type::Float64)),
                Box::new(number(span, 1.0)),
            ),
        },
    ));
    vec![
        S::Init(at, number(span, 0.0)),
        S::While(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(
                    ast::BinaryOp::Lt,
                    Box::new(local(span, at, Type::Float64)),
                    Box::new(length),
                ),
            },
            body,
        ),
    ]
}
impl Lower<'_> {
    pub(super) fn object_assign(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if self.binding("Object").is_ok() || self.functions.contains_key("Object") {
            return Err(unsupported("shadowed Object.assign intrinsic"));
        }
        if call.type_args.is_some() {
            return Err(invalid("Object.assign has no type arguments"));
        }
        let span = call.span;
        let mut roots = Vec::new();
        let mut body = Vec::new();
        let mut arguments = Vec::new();
        if let Some(target) = call.args.first() {
            self.reject_readonly_write(&target.expr)?;
        }
        for arg in &call.args {
            if arg.spread.is_some() {
                return Err(unsupported("Object.assign argument spread"));
            }
            let value = self.expr(&arg.expr)?;
            let value = if value.ty == Type::Void {
                Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Undefined(Box::new(value)),
                }
            } else {
                value
            };
            let ty = value.ty;
            let slot = self.hidden(ty);
            roots.push(slot);
            body.push(S::Init(slot, value));
            arguments.push((slot, ty));
        }
        if arguments.is_empty() {
            let slot = self.hidden(Type::Undefined);
            body.push(S::Init(
                slot,
                Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                },
            ));
            arguments.push((slot, Type::Undefined));
        }
        // A fresh literal has no escaped identity yet. Reserve the statically known
        // record-source fields in its original allocation; never migrate an alias.
        if call
            .args
            .first()
            .is_some_and(|arg| matches!(stmt::unparen(&arg.expr), ast::Expr::Object(_)))
        {
            let source_types: Vec<_> = arguments.iter().skip(1).map(|(_, ty)| *ty).collect();
            if let Some(S::Init(slot, value)) = body.first_mut() {
                let placeholder = Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                };
                let original = std::mem::replace(value, placeholder);
                *value = self.assign_literal_target(original, &source_types)?;
                arguments[0].1 = value.ty;
                self.local_types.insert(*slot, value.ty);
            }
        }
        let (target, target_ty) = arguments[0];
        let members = unions::members(target_ty, self.types);
        let targets: Vec<_> = members
            .iter()
            .copied()
            .filter(|t| !matches!(t, Type::Null | Type::Undefined))
            .collect();
        if targets
            .iter()
            .any(|t| !matches!(t, Type::Record(_) | Type::Array(_)))
        {
            return Err(unsupported(
                "Object.assign primitive target boxing is not implemented",
            ));
        }
        let result = if targets.is_empty() {
            Type::Undefined
        } else {
            unions::intern(targets.clone(), self.types)?
        };
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        roots.push(output);
        if members
            .iter()
            .any(|t| matches!(t, Type::Null | Type::Undefined))
        {
            let error = self.array_error(
                varta_common::value::ErrorClass::TypeError,
                "Cannot convert undefined or null to object",
                span,
            )?;
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsNullish(Box::new(local(span, target, target_ty))),
                },
                vec![S::Throw(error)],
                vec![],
            ));
        }
        for ty in targets.iter().copied() {
            let destination = self.hidden(ty);
            let mut branch = vec![S::Init(destination, selected(span, target, target_ty, ty))];
            for &(source, source_ty) in arguments.iter().skip(1) {
                for from in unions::members(source_ty, self.types) {
                    if matches!(
                        from,
                        Type::Null
                            | Type::Undefined
                            | Type::Float64
                            | Type::Boolean
                            | Type::BigInt
                            | Type::Function(_)
                    ) || from.is_error()
                    {
                        continue;
                    }
                    if !matches!(from, Type::Record(_) | Type::Array(_) | Type::String) {
                        return Err(unsupported("Object.assign source value kind"));
                    }
                    let prepared = self.hidden(from);
                    let mut copy = vec![S::Init(prepared, selected(span, source, source_ty, from))];
                    copy.extend(self.assign_source(span, destination, ty, prepared, from)?);
                    let copy = vec![S::Scope(vec![prepared], copy)];
                    if source_ty.is_union() {
                        branch.push(S::If(
                            Expr {
                                span,
                                ty: Type::Boolean,
                                kind: E::IsUnionMember(
                                    Box::new(local(span, source, source_ty)),
                                    from,
                                ),
                            },
                            copy,
                            vec![],
                        ));
                    } else {
                        branch.extend(copy);
                    }
                }
            }
            branch.push(S::Set(
                output,
                unions::widen(local(span, destination, ty), Some(result), self.types)?,
            ));
            let branch = vec![S::Scope(vec![destination], branch)];
            if targets.len() > 1 {
                body.push(S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsUnionMember(Box::new(local(span, target, target_ty)), ty),
                    },
                    branch,
                    vec![],
                ));
            } else {
                body.extend(branch);
            }
        }
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
    fn assign_literal_target(&mut self, value: Expr, sources: &[Type]) -> Result<Expr> {
        let Type::Record(old) = value.ty else {
            return Ok(value);
        };
        if sources
            .iter()
            .flat_map(|ty| unions::members(*ty, self.types))
            .any(|ty| {
                !matches!(
                    ty,
                    Type::Record(_)
                        | Type::Float64
                        | Type::Boolean
                        | Type::BigInt
                        | Type::Null
                        | Type::Undefined
                        | Type::Function(_)
                ) && !ty.is_error()
            })
        {
            return Ok(value);
        }
        let mut fields = self.types.records[old as usize].fields.clone();
        for ty in sources
            .iter()
            .flat_map(|ty| unions::members(*ty, self.types))
            .collect::<Vec<_>>()
        {
            let Type::Record(id) = ty else {
                continue;
            };
            for incoming in self.types.records[id as usize].fields.clone() {
                if let Some(existing) = fields.iter_mut().find(|f| f.name == incoming.name) {
                    let members = unions::members(existing.ty, self.types)
                        .into_iter()
                        .chain(unions::members(incoming.ty, self.types))
                        .collect();
                    existing.ty = unions::intern(members, self.types)?;
                } else {
                    let mut members = unions::members(incoming.ty, self.types);
                    members.push(Type::Undefined);
                    fields.push(ir::RecordField {
                        name: incoming.name,
                        ty: unions::intern(members, self.types)?,
                        optional: true,
                    });
                }
            }
        }
        let ty = records::intern(fields, self.types)?;
        if ty == value.ty {
            return Ok(value);
        }
        let Type::Record(id) = ty else {
            return Err(invalid("Object.assign literal shape"));
        };
        let old_fields = self.types.records[old as usize].fields.clone();
        let span = value.span;
        let E::Record(initializers) = value.kind else {
            return Err(unsupported("Object.assign literal target form"));
        };
        let mut values = Vec::new();
        for (slot, value) in initializers {
            let name = old_fields[slot as usize].name.clone();
            let expected = self.types.records[id as usize]
                .fields
                .iter()
                .find(|f| f.name == name)
                .ok_or_else(|| invalid("Object.assign literal field"))?
                .ty;
            values.push((name, unions::widen(value, Some(expected), self.types)?));
        }
        self.optional_record(span, id, values)
    }
    fn assign_source(
        &mut self,
        span: swc_common::Span,
        target: u32,
        target_ty: Type,
        source: u32,
        source_ty: Type,
    ) -> Result<Vec<S>> {
        if let Type::Record(shape) = source_ty {
            let keys = self.enumerate_value(local(span, source, source_ty), Kind::Keys, span)?;
            let keys_ty = keys.ty;
            let key_array = self.hidden(keys_ty);
            let at = self.hidden(Type::Float64);
            let key = self.hidden(Type::String);
            let fields = self.types.records[shape as usize].fields.clone();
            let mut entries = Vec::new();
            for (slot, field) in fields.into_iter().enumerate() {
                let value = Expr {
                    span,
                    ty: field.ty,
                    kind: E::Field(Box::new(local(span, source, source_ty)), slot as u32),
                };
                let write = if let Type::Record(destination) = target_ty {
                    self.assign_record_field(span, target, destination, &field.name, value)?
                } else {
                    self.assign_array_named(span, target, target_ty, &field.name, value)?
                };
                entries.push((field.name, vec![write]));
            }
            entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            let mut entries: Vec<_> = entries
                .into_iter()
                .map(|(name, body)| (self.types.profile.string_bytes(&name), body))
                .collect();
            let mut step = vec![S::Set(
                key,
                index(span, key_array, keys_ty, at, Type::String),
            )];
            step.extend(own_keys::lookup(span, key, &mut entries));
            let mut body = vec![S::Init(key_array, keys)];
            body.extend(copy_loop(
                span,
                at,
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Length(Box::new(local(span, key_array, keys_ty))),
                },
                step,
            ));
            return Ok(vec![S::Scope(vec![key_array, at, key], body)]);
        }
        if let Type::Array(id) = target_ty {
            let (array, array_ty, element, mut body) = if let Type::Array(id) = source_ty {
                (
                    source,
                    source_ty,
                    self.types.arrays[id as usize],
                    Vec::new(),
                )
            } else {
                let values =
                    self.enumerate_value(local(span, source, source_ty), Kind::Values, span)?;
                let ty = values.ty;
                let slot = self.hidden(ty);
                (slot, ty, Type::String, vec![S::Init(slot, values)])
            };
            let at = self.hidden(Type::Float64);
            let length = self.hidden(Type::Float64);
            body.push(S::Init(
                length,
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Length(Box::new(local(span, array, array_ty))),
                },
            ));
            let value = unions::widen(
                index(span, array, array_ty, at, element),
                Some(self.types.arrays[id as usize]),
                self.types,
            )?;
            let store = S::SetIndex {
                array: local(span, target, target_ty),
                index: local(span, at, Type::Float64),
                value,
                slots: Some((self.hidden(target_ty), self.hidden(Type::Float64))),
            };
            body.extend(copy_loop(
                span,
                at,
                local(span, length, Type::Float64),
                vec![S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::ArrayHasOwn(
                            Box::new(local(span, array, array_ty)),
                            Box::new(local(span, at, Type::Float64)),
                        ),
                    },
                    vec![store],
                    vec![],
                )],
            ));
            let mut roots = vec![at, length];
            if array != source {
                roots.push(array);
            }
            return Ok(vec![S::Scope(roots, body)]);
        }
        let keys = self.enumerate_value(local(span, source, source_ty), Kind::Keys, span)?;
        let values = self.enumerate_value(local(span, source, source_ty), Kind::Values, span)?;
        let keys_ty = keys.ty;
        let values_ty = values.ty;
        let Type::Array(values_id) = values_ty else {
            return Err(invalid("Object.assign values type"));
        };
        let element = self.types.arrays[values_id as usize];
        let key_array = self.hidden(keys_ty);
        let value_array = self.hidden(values_ty);
        let at = self.hidden(Type::Float64);
        let Type::Record(shape) = target_ty else {
            return Err(invalid("Object.assign target type"));
        };
        let write = self.checked_record_write_value(
            span,
            local(span, target, target_ty),
            index(span, key_array, keys_ty, at, Type::String),
            index(span, value_array, values_ty, at, element),
            shape,
        )?;
        let mut body = vec![S::Init(key_array, keys), S::Init(value_array, values)];
        body.extend(copy_loop(
            span,
            at,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Length(Box::new(local(span, key_array, keys_ty))),
            },
            vec![S::Expr(write)],
        ));
        Ok(vec![S::Scope(vec![key_array, value_array, at], body)])
    }
    fn assign_array_named(
        &mut self,
        span: swc_common::Span,
        target: u32,
        ty: Type,
        key: &str,
        value: Expr,
    ) -> Result<S> {
        if key == "length" {
            if value.ty != Type::Float64 {
                return Err(type_error(
                    "Object.assign array length requires a Number field",
                ));
            }
            return Ok(S::ArrayLength {
                array: local(span, target, ty),
                length: value,
                error: self.array_error(
                    varta_common::value::ErrorClass::RangeError,
                    "Invalid array length",
                    span,
                )?,
            });
        }
        let at = key
            .parse::<u32>()
            .ok()
            .filter(|n| n.to_string() == key && *n != u32::MAX)
            .ok_or_else(|| {
                unsupported("Object.assign cannot add non-index checked array properties")
            })?;
        let Type::Array(id) = ty else {
            return Err(invalid("Object.assign array target"));
        };
        let expected = self.types.arrays[id as usize];
        Ok(S::SetIndex {
            array: local(span, target, ty),
            index: number(span, f64::from(at)),
            value: unions::widen(value, Some(expected), self.types)?,
            slots: Some((self.hidden(ty), self.hidden(Type::Float64))),
        })
    }
}
