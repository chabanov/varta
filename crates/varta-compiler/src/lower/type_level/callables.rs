//! Callable objects with properties (v38 `type_level`, step 4; coordinator
//! decision under the owner's delegated authority: functions are objects,
//! so function values get their own property storage).
//!
//! A type with call signatures and other members (`interface Counter { ():
//! number; count: number }`), an intersection of a function type with an
//! object type, `Object.assign(fn, …)` and a function with assigned
//! properties is a function type marked with its properties
//! (`FunctionType::properties`): the dictionary type of a mixed record of
//! `unknown` whose declared fields are the properties. Physically it is the
//! function: every value18 closure has a property storage edge
//! (`function::PROPERTIES_OFFSET`), a dictionary storage record created on
//! the first property access, so any function value (a named function, an
//! arrow, a class value) can hold own properties, as in JavaScript. A
//! property access of a callable object reads and writes that dictionary
//! through the declared view (`Lower::function_property_storage`): a
//! declared read converts the stored `unknown` with a checked test, and
//! `Object.keys`, `in`, `delete` and `for…in` see the own properties in
//! insertion order. `typeof` stays `"function"`, JSON skips the function,
//! and identity is the function's. `name` and `length` are the function's
//! own non-writable properties (track D1b), never storage entries.
use super::*;
use ir::RecordField;

/// The function's own properties that are not storage entries.
const INTRINSIC_PROPERTIES: [&str; 2] = ["name", "length"];

/// The function type `call` with the own properties `fields` (added to any
/// it already declares).
pub(in crate::lower) fn with_properties(
    call: Type,
    fields: Vec<RecordField>,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let Type::Function(id) = call else {
        return Err(invalid("missing callable function type"));
    };
    let mut signature = types.functions[id as usize].clone();
    if signature.construct {
        return Err(unsupported(
            "construct signature together with properties (a class's static members are not \
             properties of its constructor value)",
        ));
    }
    let mut merged: Vec<RecordField> = match signature.properties {
        Some(view) => types
            .declared(Type::Dictionary(view))
            .map(|members| members.fields.clone())
            .unwrap_or_default(),
        None => Vec::new(),
    };
    for field in fields {
        if INTRINSIC_PROPERTIES.contains(&field.name.as_str()) {
            return Err(unsupported(&format!(
                "declared property {} of a callable object (a function's own {} is not writable)",
                field.name, field.name
            )));
        }
        match merged
            .iter_mut()
            .find(|existing| existing.name == field.name)
        {
            Some(existing) => {
                existing.ty = objects::intersect(existing.ty, field.ty, types)?;
                existing.optional &= field.optional;
            }
            None => merged.push(field),
        }
    }
    if merged.is_empty() {
        return Ok(call);
    }
    // A callable object has no index signature: its keys are exactly the
    // declared names (the keyed view of `index_keys`).
    let names = merged.iter().map(|field| field.name.clone()).collect();
    let Type::Literal(_, keys) = objects::string_keys(names, types)? else {
        return Err(invalid("missing callable property names"));
    };
    let Type::Dictionary(view) =
        mixed_records::intern_keyed(Type::Unknown, merged, false, Some(keys), types)?
    else {
        return Err(invalid("missing callable property view"));
    };
    signature.properties = Some(view);
    if let Some(index) = types.functions.iter().position(|f| *f == signature) {
        return Ok(Type::Function(index as u32));
    }
    if types.functions.len() >= types.profile.limits().function_types {
        return Err(invalid("function type count exceeds limit"));
    }
    types.functions.push(signature);
    Ok(Type::Function(types.functions.len() as u32 - 1))
}

/// The declared property view of a callable object type, if any.
pub(in crate::lower) fn properties_of(ty: Type, types: &ir::ValueTypes) -> Option<Type> {
    match ty {
        Type::Function(id) => types.functions[id as usize]
            .properties
            .map(Type::Dictionary),
        _ => None,
    }
}

/// The operands of an identity comparison of two function values whose
/// types differ only in their markers (own properties, overloads): the right
/// one retyped to the left's type, since both are one physical function type.
pub(in crate::lower) fn function_operands(
    op: ast::BinaryOp,
    left: Expr,
    right: Expr,
    types: &ir::ValueTypes,
) -> (Expr, Expr) {
    let (Type::Function(a), Type::Function(b)) = (left.ty, right.ty) else {
        return (left, right);
    };
    let identity = matches!(
        op,
        ast::BinaryOp::EqEqEq | ast::BinaryOp::NotEqEq | ast::BinaryOp::EqEq | ast::BinaryOp::NotEq
    );
    let physical = |f: &ir::FunctionType| (f.receiver, f.params.clone(), f.shape.clone(), f.result);
    if !types.profile.type_level()
        || !identity
        || a == b
        || physical(&types.functions[a as usize]) != physical(&types.functions[b as usize])
    {
        return (left, right);
    }
    let ty = left.ty;
    let right = Expr {
        span: right.span,
        ty,
        kind: E::Reinterpret(Box::new(right)),
    };
    (left, right)
}

/// The context of a function literal expected as `expected`: a callable
/// object type with a required property is not the literal's own type
/// (tsc: TS2741), so the literal takes its call signature and converting it
/// then fails; other types are kept.
pub(in crate::lower) fn callable_context(
    expected: Option<Type>,
    types: &mut ir::ValueTypes,
) -> Result<Option<Type>> {
    let Some(Type::Function(id)) = expected else {
        return Ok(expected);
    };
    let mut signature = types.functions[id as usize].clone();
    let Some(view) = signature.properties else {
        return Ok(expected);
    };
    let required = types
        .declared(Type::Dictionary(view))
        .is_some_and(|members| members.fields.iter().any(|field| !field.optional));
    if !required {
        return Ok(expected);
    }
    signature.properties = None;
    if let Some(index) = types.functions.iter().position(|f| *f == signature) {
        return Ok(Some(Type::Function(index as u32)));
    }
    if types.functions.len() >= types.profile.limits().function_types {
        return Err(invalid("function type count exceeds limit"));
    }
    types.functions.push(signature);
    Ok(Some(Type::Function(types.functions.len() as u32 - 1)))
}

/// Whether two function types differ only in their own properties (a type
/// assertion between them retypes the function, as tsc relates them).
pub(in crate::lower) fn same_call(given: Type, wanted: Type, types: &ir::ValueTypes) -> bool {
    let (Type::Function(from), Type::Function(into)) = (given, wanted) else {
        return false;
    };
    let (from, into) = (
        &types.functions[from as usize],
        &types.functions[into as usize],
    );
    from.properties != into.properties
        && ir::FunctionType {
            properties: None,
            ..from.clone()
        } == ir::FunctionType {
            properties: None,
            ..into.clone()
        }
}

/// Whether a function value of type `given` may be used as `wanted` by
/// retyping: the same call signature, and every property `wanted` requires
/// declared by `given` with an assignable type (an optional one may be
/// missing).
pub(in crate::lower) fn converts(given: Type, wanted: Type, types: &ir::ValueTypes) -> bool {
    let (Type::Function(from), Type::Function(into)) = (given, wanted) else {
        return false;
    };
    let (from, into) = (
        &types.functions[from as usize],
        &types.functions[into as usize],
    );
    if from.properties == into.properties
        || from.receiver != into.receiver
        || from.params != into.params
        || from.shape != into.shape
        || from.result != into.result
        || from.construct != into.construct
        || from.overloads != into.overloads
        || from.generic != into.generic
    {
        return false;
    }
    let fields = |view: Option<u32>| -> Vec<RecordField> {
        view.and_then(|view| types.declared(Type::Dictionary(view)))
            .map(|members| members.fields.clone())
            .unwrap_or_default()
    };
    let (own, required) = (fields(from.properties), fields(into.properties));
    required
        .iter()
        .all(|field| match own.iter().find(|f| f.name == field.name) {
            Some(declared) => {
                (field.optional || !declared.optional)
                    && mixed_records::assignable(field.ty, declared.ty, types)
            }
            None => field.optional,
        })
}

impl Lower<'_> {
    /// The own-property storage of the function `value`, typed as the
    /// declared view `view`: the closure's dictionary, created empty and
    /// installed on first use.
    pub(in crate::lower) fn function_property_storage(
        &mut self,
        value: Expr,
        view: Type,
    ) -> Result<Expr> {
        let span = value.span;
        let plain = dictionaries::intern(Type::Unknown, self.types)?;
        let Type::Dictionary(plain_id) = plain else {
            return Err(invalid("missing property dictionary"));
        };
        let dictionary = self.types.dictionaries[plain_id as usize];
        let function = self.hidden(value.ty);
        let storage = self.hidden(plain);
        let output = self.hidden(view);
        let marker = self.hidden(Type::Number);
        let local = |slot: u32, ty: Type| Expr {
            span,
            ty,
            kind: E::Local(slot),
        };
        let empty = self.dictionary_storage(
            span,
            plain,
            Expr {
                span,
                ty: dictionary.keys,
                kind: E::Array(vec![]),
            },
            Expr {
                span,
                ty: dictionary.values,
                kind: E::Array(vec![]),
            },
        )?;
        let absent = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Binary(
                ast::BinaryOp::EqEqEq,
                Box::new(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Intrinsic(ir::Intrinsic::Identity, vec![local(storage, plain)]),
                }),
                Box::new(Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Float64(0.0),
                }),
            ),
        };
        let install = Expr {
            span,
            ty: Type::Void,
            kind: E::Intrinsic(
                ir::Intrinsic::SetFunctionProperties,
                vec![local(function, value.ty), local(storage, plain)],
            ),
        };
        let read = Expr {
            span,
            ty: plain,
            kind: E::Intrinsic(
                ir::Intrinsic::FunctionProperties,
                vec![local(function, value.ty)],
            ),
        };
        Ok(Expr {
            span,
            ty: view,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![function, storage],
                body: vec![
                    S::Init(function, value),
                    S::Init(storage, read),
                    S::If(
                        absent,
                        vec![S::Set(storage, empty), S::Expr(install)],
                        Vec::new(),
                    ),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty: view,
                            kind: E::Reinterpret(Box::new(local(storage, plain))),
                        },
                    ),
                ],
            },
        })
    }

    /// `Object.assign(fn, ...sources)` (step 4): the own enumerable
    /// properties of each source are defined on the function in order, and
    /// the result is the function itself, typed as a callable object with
    /// the sources' properties (TypeScript's `typeof fn & S`). `None` when
    /// the target is not a function.
    pub(in crate::lower) fn function_assign(
        &mut self,
        call: &ast::CallExpr,
    ) -> Result<Option<Expr>> {
        let Some(first) = call.args.first() else {
            return Ok(None);
        };
        let literal = match stmt::unparen(&first.expr) {
            ast::Expr::Arrow(_) | ast::Expr::Fn(_) => true,
            // A module function by name.
            ast::Expr::Ident(id) => {
                self.binding(id.sym.as_ref()).is_err()
                    && self.functions.contains_key(id.sym.as_ref())
            }
            _ => false,
        };
        if !self.types.profile.type_level()
            || !literal && !matches!(self.peek_type(&first.expr, 0), Some(Type::Function(_)))
        {
            return Ok(None);
        }
        let span = call.span;
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self
                .source
                .locate(unsupported("Object.assign argument spread"), span));
        }
        let target = self.expr(&first.expr)?;
        let target_ty = target.ty;
        if !matches!(target_ty, Type::Function(_)) {
            return Ok(None);
        }
        let plain = dictionaries::intern(Type::Unknown, self.types)?;
        let local = |slot: u32, ty: Type| Expr {
            span,
            ty,
            kind: E::Local(slot),
        };
        let function = self.hidden(target_ty);
        let storage = self.hidden(plain);
        let mut roots = vec![function, storage];
        let mut body = vec![S::Init(function, target)];
        let mut fields = Vec::new();
        let mut sources = Vec::new();
        for arg in &call.args[1..] {
            let source = self.expr(&arg.expr)?;
            match source.ty {
                Type::Null | Type::Undefined => {
                    body.push(S::Expr(source));
                    continue;
                }
                Type::Record(shape) if self.types.records[shape as usize].class.is_none() => {
                    fields.extend(self.types.records[shape as usize].fields.clone());
                }
                other => {
                    return Err(self.source.locate(
                        unsupported(&format!(
                            "Object.assign onto a function from {} (only object literals and                              closed records have statically known properties)",
                            subtyping::describe(other, self.types)
                        )),
                        source.span,
                    ));
                }
            }
            // Evaluate every argument before reading any source's own
            // properties, as a call to Object.assign does.
            let ty = source.ty;
            let slot = self.hidden(ty);
            roots.push(slot);
            body.push(S::Init(slot, source));
            sources.push((slot, ty));
        }
        let installed = self.function_property_storage(local(function, target_ty), plain)?;
        body.push(S::Init(storage, installed));
        for (source, ty) in sources {
            let (_, held, copied) =
                self.spread_entries(local(source, ty), local(storage, plain), &[])?;
            roots.extend(held);
            body.extend(copied);
            // String-key enumeration deliberately omits symbols. Copy them
            // afterwards through the shared Object.assign symbol path.
            body.extend(self.assign_symbols(span, (storage, plain), (source, ty), &mut roots)?);
        }
        let ty = with_properties(target_ty, fields, self.types)
            .map_err(|e| self.source.locate(e, span))?;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        roots.push(output);
        body.push(S::Init(
            output,
            Expr {
                span,
                ty,
                kind: E::Reinterpret(Box::new(local(function, target_ty))),
            },
        ));
        Ok(Some(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        }))
    }

    /// The value of the module function `name` (kept out of the expression
    /// lowering frame).
    pub(in crate::lower) fn named_function_value(
        &mut self,
        name: &str,
        signature: &Signature,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let value = self.function_reference(signature, span)?;
        self.expando_value(name, value)
    }

    /// The value of a module function `name`: with its expando properties
    /// (`f.count = 0;` at module level), a callable object.
    fn expando_value(&mut self, name: &str, value: Expr) -> Result<Expr> {
        if !self.types.profile.type_level() {
            return Ok(value);
        }
        let Some((module, declared)) = queries::declaring(name, self.types) else {
            return Ok(value);
        };
        let fields = queries::expando_fields(module, &declared, self.types)
            .map_err(|e| self.source.locate(e, value.span))?;
        if fields.is_empty() {
            return Ok(value);
        }
        let ty = with_properties(value.ty, fields, self.types)
            .map_err(|e| self.source.locate(e, value.span))?;
        Ok(Expr {
            span: value.span,
            ty,
            kind: E::Reinterpret(Box::new(value)),
        })
    }

    /// A function value enumerated or tested as an object (`Object.keys`,
    /// `Object.entries`, `in`, `for…in`): its property storage (an empty
    /// one for a function without own properties); other values unchanged.
    pub(in crate::lower) fn function_object(&mut self, value: Expr) -> Result<Expr> {
        if !self.types.profile.type_level() || !matches!(value.ty, Type::Function(_)) {
            return Ok(value);
        }
        let plain = dictionaries::intern(Type::Unknown, self.types)?;
        let storage = self.function_property_storage(value, plain)?;
        self.string_keyed(storage)
    }

    /// A fresh dictionary of the string-keyed entries of the own-property
    /// storage `storage`, in its own-key order: the string-key enumeration
    /// of `Object.keys`, `Object.entries`, `Object.values`, `for…in` and
    /// `in`, which skips symbol-keyed properties (their member keys,
    /// `modules::symbol_keys`, start with the private-key prefix and `[`).
    fn string_keyed(&mut self, storage: Expr) -> Result<Expr> {
        let span = storage.span;
        let ty = storage.ty;
        let Type::Dictionary(id) = ty else {
            return Err(invalid("missing property storage"));
        };
        let dictionary = self.types.dictionaries[id as usize];
        let local = |slot: u32, ty: Type| Expr {
            span,
            ty,
            kind: E::Local(slot),
        };
        let number = |value: f64| Expr {
            span,
            ty: Type::Float64,
            kind: E::Float64(value),
        };
        let source = self.hidden(ty);
        let output = self.hidden(ty);
        let index = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        let empty = self.dictionary_storage(
            span,
            ty,
            Expr {
                span,
                ty: dictionary.keys,
                kind: E::Array(vec![]),
            },
            Expr {
                span,
                ty: dictionary.values,
                kind: E::Array(vec![]),
            },
        )?;
        let vector = |slot: u32, ty: Type| Expr {
            span,
            ty,
            kind: E::Field(Box::new(local(source, ty)), slot),
        };
        let at = || local(index, Type::Float64);
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::Index(
                Box::new(vector(dictionary.keys_slot(), dictionary.keys)),
                Box::new(at()),
            ),
        };
        let value = Expr {
            span,
            ty: dictionary.value,
            kind: E::Index(
                Box::new(vector(dictionary.values_slot(), dictionary.values)),
                Box::new(at()),
            ),
        };
        let prefix = format!("{}[", varta_common::value::PRIVATE_KEY_PREFIX);
        let symbol = Expr {
            span,
            ty: Type::Boolean,
            kind: E::StringSearch(
                ir::StringSearch::StartsWith,
                Box::new(key.clone()),
                Box::new(Expr {
                    span,
                    ty: Type::String,
                    kind: E::String(self.types.profile.string_bytes(&prefix)),
                }),
                Box::new(number(0.0)),
            ),
        };
        let copy = self.dictionary_set(local(output, ty), key, value, dictionary.value)?;
        let length = Expr {
            span,
            ty: Type::Float64,
            kind: E::Length(Box::new(vector(dictionary.keys_slot(), dictionary.keys))),
        };
        let mut body = vec![S::Init(source, storage), S::Init(output, empty)];
        body.extend(self.dictionary_compact(local(source, ty))?);
        body.push(S::Init(index, number(0.0)));
        body.push(S::While(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Binary(ast::BinaryOp::Lt, Box::new(at()), Box::new(length)),
            },
            vec![
                S::If(symbol, Vec::new(), vec![S::Expr(copy)]),
                S::Set(
                    index,
                    Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Binary(ast::BinaryOp::Add, Box::new(at()), Box::new(number(1.0))),
                    },
                ),
            ],
        ));
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![source, output],
                body,
            },
        })
    }

    /// The base of a member access `f.name` of a callable object: its
    /// property storage, through which every read, write, update and
    /// `delete` goes. `name` and `length` are the function's own properties
    /// (track D1b), never storage entries.
    pub(in crate::lower) fn callable_member(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
    ) -> Result<Expr> {
        let Some(view) = properties_of(value.ty, self.types) else {
            return Ok(value);
        };
        if let Some(name) = mixed_records::static_key(member)
            && INTRINSIC_PROPERTIES.contains(&name.as_str())
        {
            return Err(self.source.locate(
                unsupported(&format!(
                    "the {name} of a function value (the function's own {name} property)"
                )),
                member.prop.span(),
            ));
        }
        self.function_property_storage(value, view)
    }
}
