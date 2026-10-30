//! Engine collections of v37 (`collections`): `Map<K, V>`, `Set<T>` and
//! `Uint8Array` as nominal types (`Type::Collection`) over private storage.
//! A map or set is the `MapStore<K, V>`/`SetStore<T>` record of the
//! collections prelude, instantiated through the prelude's own generic
//! interfaces, so the prelude source is the single layout definition; a
//! `Uint8Array` is a dense `number[]` of byte values. The types are erased to
//! their storage before emission and add no value tag: logging, JSON and
//! `unknown` reject them (their storage is private), as v34 did for promises.
//! Operations call the generic prelude functions (`lower::engine`).
use super::*;
use crate::modules::engine::{BINARY, COLLECTIONS};
use ir::CollectionKind as K;
use std::collections::BTreeSet;

/// `Uint8Array.BYTES_PER_ELEMENT` and `byteOffset` (no shared buffers).
const BYTES_PER_ELEMENT: f64 = 1.0;
const BYTE_OFFSET: f64 = 0.0;

/// Hash-key types: primitives hash by value, references by identity.
fn admitted_key(key: Type) -> bool {
    matches!(
        key,
        Type::String | Type::Float64 | Type::Boolean | Type::BigInt
    ) || key.is_handle() && !key.is_cell() && key != Type::Unknown
}

/// The collection kind a built-in type name denotes, unless a declaration
/// shadows it.
pub(super) fn builtin(name: &str, types: &ir::ValueTypes) -> Option<K> {
    if !types.profile.collections() || named::declared(types, name) {
        return None;
    }
    Some(match name {
        "Map" | "ReadonlyMap" => K::Map,
        "Set" | "ReadonlySet" => K::Set,
        "Uint8Array" => K::Uint8Array,
        _ => return None,
    })
}

/// `Map<K, V>`, `Set<T>`, `ReadonlyMap`/`ReadonlySet` and `Uint8Array`.
pub(super) fn annotation(
    reference: &ast::TsTypeRef,
    kind: K,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let params: Vec<&ast::TsType> = reference
        .type_params
        .as_ref()
        .map(|p| p.params.iter().map(|ty| &**ty).collect())
        .unwrap_or_default();
    match (kind, params.as_slice()) {
        (K::Map, [key, value]) => {
            let key = keyword_type(key, types)?;
            let value = keyword_type(value, types)?;
            intern(K::Map, key, value, types)
        }
        (K::Set, [value]) => {
            let value = keyword_type(value, types)?;
            intern(K::Set, value, value, types)
        }
        // `Uint8Array<ArrayBuffer>` (TypeScript 5.7) names the buffer type.
        (K::Uint8Array, [])
        | (
            K::Uint8Array,
            [
                ast::TsType::TsTypeRef(ast::TsTypeRef {
                    type_name: ast::TsEntityName::Ident(_),
                    type_params: None,
                    ..
                }),
            ],
        ) => {
            if let [ast::TsType::TsTypeRef(buffer)] = params.as_slice()
                && let ast::TsEntityName::Ident(name) = &buffer.type_name
                && !matches!(name.sym.as_ref(), "ArrayBuffer" | "ArrayBufferLike")
            {
                return Err(unsupported("Uint8Array buffer type argument"));
            }
            bytes(types)
        }
        _ => Err(invalid(format!(
            "{} type argument count mismatch",
            kind.name()
        ))),
    }
}

/// The `Uint8Array` type.
pub(super) fn bytes(types: &mut ir::ValueTypes) -> Result<Type> {
    let number = types.number();
    intern(K::Uint8Array, number, number, types)
}

pub(super) fn intern(kind: K, key: Type, value: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    // Literal types are compile-time sets of their base primitive.
    let key = key.erased();
    let value = if kind == K::Set { key } else { value };
    if kind != K::Uint8Array && !admitted_key(key) {
        return Err(unsupported(&format!(
            "{} key type {key}; keys are strings, numbers, booleans, bigints or objects",
            kind.name()
        )));
    }
    if kind == K::Map && (value == Type::Void || value.width() > 1) {
        return Err(unsupported(&format!("Map value type {value}")));
    }
    if let Some(id) = types
        .collections
        .iter()
        .position(|c| c.kind == kind && c.key == key && c.value == value)
    {
        return Ok(Type::Collection(kind, id as u32));
    }
    let storage = match kind {
        K::Set => store(types, "SetStore", &[key])?,
        K::Map => store(types, "MapStore", &[key, value])?,
        K::Uint8Array => arrays::intern(types.number(), types)?,
    };
    let id = types.collections.len() as u32;
    types.collections.push(ir::CollectionType {
        kind,
        key,
        value,
        storage,
    });
    Ok(Type::Collection(kind, id))
}

/// The prelude's generic storage interface `name` instantiated with `args`.
fn store(types: &mut ir::ValueTypes, name: &str, args: &[Type]) -> Result<Type> {
    let module = engine::module(&COLLECTIONS, types)
        .ok_or_else(|| CompilerError::Internal("the collections prelude is not loaded".into()))?;
    let span = swc_common::DUMMY_SP;
    let reference = |name: &str, params: Option<Vec<ast::TsType>>| {
        ast::TsType::TsTypeRef(ast::TsTypeRef {
            span,
            type_name: ast::TsEntityName::Ident(ast::Ident::new(
                name.into(),
                span,
                Default::default(),
            )),
            type_params: params.map(|params| {
                Box::new(ast::TsTypeParamInstantiation {
                    span,
                    params: params.into_iter().map(Box::new).collect(),
                })
            }),
        })
    };
    const PARAMETERS: [&str; 2] = ["K", "V"];
    let frame: generics::Frame = PARAMETERS
        .iter()
        .zip(args)
        .map(|(name, ty)| ((*name).to_owned(), *ty))
        .collect();
    let target = reference(
        name,
        Some(
            PARAMETERS[..args.len()]
                .iter()
                .map(|name| reference(name, None))
                .collect(),
        ),
    );
    let caller = named::current(types);
    named::enter(types, module);
    let result = generics::with_frame(types, frame, |types| keyword_type(&target, types));
    named::enter(types, caller);
    result
}

/// Whether a value of `ty` may reach collection storage, which no host reader
/// may see (logging, JSON, `unknown`).
pub(super) fn reaches(ty: Type, types: &ir::ValueTypes) -> bool {
    fn visit(ty: Type, types: &ir::ValueTypes, seen: &mut BTreeSet<Type>) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match ty {
            Type::Collection(..) => true,
            Type::Class(id) if types.classes.get(id).opaque => true,
            Type::Class(id) => visit(Type::Record(types.classes.shape(id)), types, seen),
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .any(|f| visit(f.ty, types, seen)),
            Type::Array(id) => visit(types.arrays[id as usize], types, seen),
            Type::Tuple(id) => types.tuples[id as usize]
                .elements
                .iter()
                .any(|ty| visit(*ty, types, seen)),
            Type::Dictionary(id) => visit(types.dictionaries[id as usize].value, types, seen),
            Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
                .iter()
                .any(|t| visit(*t, types, seen)),
            Type::Promise(id) => visit(types.promises[id as usize].value, types, seen),
            _ => false,
        }
    }
    types.profile.collections() && visit(ty, types, &mut BTreeSet::new())
}
/// Whether a value of `ty` may reach collection storage other than a
/// `Uint8Array`'s (Map and Set storage, opaque builtin classes).
pub(super) fn reaches_private(ty: Type, types: &ir::ValueTypes) -> bool {
    fn visit(ty: Type, types: &ir::ValueTypes, seen: &mut BTreeSet<Type>) -> bool {
        if !seen.insert(ty) {
            return false;
        }
        match ty {
            Type::Collection(K::Uint8Array, _) => false,
            Type::Collection(..) => true,
            Type::Class(id) if types.classes.get(id).opaque => true,
            Type::Class(id) => visit(Type::Record(types.classes.shape(id)), types, seen),
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .any(|f| visit(f.ty, types, seen)),
            Type::Array(id) => visit(types.arrays[id as usize], types, seen),
            Type::Tuple(id) => types.tuples[id as usize]
                .elements
                .iter()
                .any(|ty| visit(*ty, types, seen)),
            Type::Dictionary(id) => visit(types.dictionaries[id as usize].value, types, seen),
            Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
                .iter()
                .any(|t| visit(*t, types, seen)),
            Type::Promise(id) => visit(types.promises[id as usize].value, types, seen),
            _ => false,
        }
    }
    visit(ty, types, &mut BTreeSet::new())
}
/// Whether `ty` is a `Uint8Array` or a union with a `Uint8Array` member: a
/// value whose own cell would hold the byte storage.
pub(super) fn holds_bytes(ty: Type, types: &ir::ValueTypes) -> bool {
    unions::members(ty, types)
        .into_iter()
        .any(|member| matches!(member, Type::Collection(K::Uint8Array, _)))
}
/// The diagnostic for a collection reaching a host value reader.
pub(super) fn opaque(what: &str) -> CompilerError {
    unsupported(&format!(
        "{what} of a value holding a Map, Set, Uint8Array or another v37 builtin object (their state is private in value layout 17)"
    ))
}

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

impl Lower<'_> {
    fn collection(&self, id: u32) -> ir::CollectionType {
        self.types.collections[id as usize]
    }
    /// The private storage of a collection value.
    pub(super) fn storage(&self, value: Expr) -> Expr {
        let Type::Collection(_, id) = value.ty else {
            return value;
        };
        Expr {
            span: value.span,
            ty: self.collection(id).storage,
            kind: E::Reinterpret(Box::new(value)),
        }
    }
    /// The generic prelude function arguments of collection `id`.
    fn type_args(&self, id: u32) -> Vec<Type> {
        let collection = self.collection(id);
        match collection.kind {
            K::Set => vec![collection.key],
            K::Map => vec![collection.key, collection.value],
            K::Uint8Array => vec![],
        }
    }
    /// Call a collections prelude function of collection `id`'s type.
    fn collection_call(
        &mut self,
        function: &str,
        id: u32,
        leading: Vec<Expr>,
        trailing: &[ast::ExprOrSpread],
        span: swc_common::Span,
    ) -> Result<Expr> {
        let prelude = if self.collection(id).kind == K::Uint8Array {
            &BINARY
        } else {
            &COLLECTIONS
        };
        let args = self.type_args(id);
        self.engine_call((prelude, function, &args), leading, trailing, span)
    }
    /// Wrap storage `result` of collection `id` as the collection value.
    fn wrap(&self, result: Expr, id: u32) -> Expr {
        let kind = self.collection(id).kind;
        Expr {
            span: result.span,
            ty: Type::Collection(kind, id),
            kind: E::Reinterpret(Box::new(result)),
        }
    }
    /// `new Map(...)`, `new Set(...)` and `new Uint8Array(...)`.
    pub(super) fn collection_new(
        &mut self,
        new: &ast::NewExpr,
        kind: K,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = new.span;
        let args = new.args.as_deref().unwrap_or_default();
        if args.iter().any(|arg| arg.spread.is_some()) || args.len() > 1 {
            return Err(self.source.locate(
                unsupported(&format!(
                    "{} constructor arguments other than one iterable",
                    kind.name()
                )),
                span,
            ));
        }
        if kind == K::Uint8Array {
            return self.bytes_new(new, args.first());
        }
        let explicit = match &new.type_args {
            Some(params) => {
                let values = params
                    .params
                    .iter()
                    .map(|ty| keyword_type(ty, self.types))
                    .collect::<Result<Vec<_>>>()
                    .map_err(|e| self.source.locate(e, params.span))?;
                Some(match (kind, values.as_slice()) {
                    (K::Map, [key, value]) => intern(K::Map, *key, *value, self.types)?,
                    (K::Set, [value]) => intern(K::Set, *value, *value, self.types)?,
                    _ => {
                        return Err(self.source.locate(
                            invalid(format!("{} type argument count mismatch", kind.name())),
                            params.span,
                        ));
                    }
                })
            }
            None => None,
        };
        let contextual = expected.and_then(|ty| {
            unions::members(ty, self.types)
                .into_iter()
                .find(|member| matches!(member, Type::Collection(k, _) if *k == kind))
        });
        let Some(source) = args.first() else {
            let Some(Type::Collection(_, id)) = explicit.or(contextual) else {
                return Err(self.source.locate(
                    type_error(format!(
                        "cannot infer the type arguments of new {}(); write new {}<...>() or annotate the binding",
                        kind.name(),
                        kind.name()
                    )),
                    span,
                ));
            };
            let create = if kind == K::Set {
                "setCreate"
            } else {
                "mapCreate"
            };
            let store = self.collection_call(create, id, vec![], &[], span)?;
            return Ok(self.wrap(store, id));
        };
        let target = explicit.or(contextual);
        let element_hint = match target {
            Some(Type::Collection(_, id)) => Some(element_hint_for(self, id)?),
            _ => None,
        };
        let value = match (target, &*source.expr) {
            // Another collection of the same kind is copied.
            (_, expression) if matches!(self.peek_type(expression, 0), Some(Type::Collection(k, _)) if k == kind) => {
                self.expr(expression)?
            }
            // `new Map([[k, v], ...])` infers `[K, V]` entries like TypeScript.
            (None, ast::Expr::Array(entries)) if kind == K::Map => self.entry_literals(entries)?,
            _ => self.expr_expected(&source.expr, element_hint)?,
        };
        if let Type::Collection(k, id) = value.ty
            && k == kind
        {
            if target.is_some_and(|ty| ty != value.ty) {
                return Err(self.source.locate(
                    type_error(format!("{} type argument mismatch", kind.name())),
                    value.span,
                ));
            }
            let copy = if kind == K::Set { "setCopy" } else { "mapCopy" };
            let storage = self.storage(value);
            let store = self.collection_call(copy, id, vec![storage], &[], span)?;
            return Ok(self.wrap(store, id));
        }
        let value = self.physical_array(value)?;
        let Type::Array(array) = value.ty else {
            return Err(self.source.locate(
                unsupported(&format!(
                    "{} constructor argument {}; pass an array or another {}",
                    kind.name(),
                    value.ty,
                    kind.name()
                )),
                value.span,
            ));
        };
        let element = self.types.arrays[array as usize];
        let ty = match target {
            Some(ty) => ty,
            None if kind == K::Set => intern(K::Set, element, element, self.types)
                .map_err(|e| self.source.locate(e, value.span))?,
            None => {
                let Type::Tuple(pair) = element else {
                    return Err(self.source.locate(
                        type_error("new Map(entries) requires an array of [key, value] tuples"),
                        value.span,
                    ));
                };
                let tuple = self.types.tuples[pair as usize].clone();
                let [key, item] = tuple.elements.as_slice() else {
                    return Err(self.source.locate(
                        type_error("new Map(entries) requires [key, value] pairs"),
                        value.span,
                    ));
                };
                intern(K::Map, *key, *item, self.types)
                    .map_err(|e| self.source.locate(e, value.span))?
            }
        };
        let Type::Collection(_, id) = ty else {
            return Err(invalid("missing collection type"));
        };
        let expected = element_hint_for(self, id)?;
        let value = unions::widen(value, Some(expected), self.types)?;
        if value.ty != expected {
            return Err(self.source.locate(
                type_error(format!(
                    "{} constructor argument type mismatch: expected {expected}, found {}",
                    kind.name(),
                    value.ty
                )),
                value.span,
            ));
        }
        let from = if kind == K::Set {
            "setFromArray"
        } else {
            "mapFromEntries"
        };
        let store = self.collection_call(from, id, vec![value], &[], span)?;
        Ok(self.wrap(store, id))
    }
    /// An array literal of `[key, value]` pair literals as `[K, V][]`, with
    /// `K` and `V` the unions of the positions' types.
    fn entry_literals(&mut self, entries: &ast::ArrayLit) -> Result<Expr> {
        let mut pairs = Vec::new();
        for entry in &entries.elems {
            let pair = match entry {
                Some(ast::ExprOrSpread { spread: None, expr }) => match stmt::unparen(expr) {
                    ast::Expr::Array(pair) => pair,
                    _ => return self.expr(&ast::Expr::Array(entries.clone())),
                },
                _ => return self.expr(&ast::Expr::Array(entries.clone())),
            };
            let [Some(key), Some(value)] = pair.elems.as_slice() else {
                return Err(self.source.locate(
                    type_error("new Map(entries) requires [key, value] pairs"),
                    pair.span,
                ));
            };
            if key.spread.is_some() || value.spread.is_some() {
                return Err(self
                    .source
                    .locate(unsupported("spread in a Map entry"), pair.span));
            }
            let key = self.expr(&key.expr)?;
            let value = self.expr(&value.expr)?;
            pairs.push((pair.span, key, value));
        }
        if pairs.is_empty() {
            return Err(self.source.locate(
                type_error("cannot infer the type arguments of new Map([]); write new Map<K, V>()"),
                entries.span,
            ));
        }
        let key = unions::intern(pairs.iter().map(|(_, k, _)| k.ty).collect(), self.types)?;
        let value = unions::intern(pairs.iter().map(|(_, _, v)| v.ty).collect(), self.types)?;
        let pair = tuples::intern(vec![key, value], tuples::PAIR, false, self.types)?;
        let Type::Tuple(tuple) = pair else {
            return Err(invalid("missing entry tuple"));
        };
        let Type::Array(physical) = self.types.tuples[tuple as usize].array else {
            return Err(invalid("missing entry array"));
        };
        let element = self.types.arrays[physical as usize];
        let mut values = Vec::new();
        for (span, k, v) in pairs {
            let k = unions::widen(
                unions::widen(k, Some(key), self.types)?,
                Some(element),
                self.types,
            )?;
            let v = unions::widen(
                unions::widen(v, Some(value), self.types)?,
                Some(element),
                self.types,
            )?;
            values.push(Some(Expr {
                span,
                ty: pair,
                kind: E::Array(vec![Some(k), Some(v)]),
            }));
        }
        Ok(Expr {
            span: entries.span,
            ty: arrays::intern(pair, self.types)?,
            kind: E::Array(values),
        })
    }
    /// A collection method call.
    pub(super) fn collection_method(
        &mut self,
        call: &ast::CallExpr,
        receiver: Expr,
        (kind, id): (K, u32),
        name: &str,
    ) -> Result<Expr> {
        let span = call.span;
        if call.type_args.is_some() {
            return Err(self.source.locate(
                invalid(format!("{}.{name} has no type arguments", kind.name())),
                span,
            ));
        }
        if kind == K::Uint8Array {
            return self.bytes_method(call, receiver, id, name);
        }
        let set = kind == K::Set;
        let function = match (set, name) {
            (true, "has") => "setHas",
            (true, "add") => "setAdd",
            (true, "delete") => "setDelete",
            (true, "clear") => "setClear",
            (true, "forEach") => "setForEach",
            (true, "values" | "keys") => "setValues",
            (true, "entries") => "setEntries",
            (false, "has") => "mapHas",
            (false, "get") => "mapGet",
            (false, "set") => "mapSet",
            (false, "delete") => "mapDelete",
            (false, "clear") => "mapClear",
            (false, "forEach") => "mapForEach",
            (false, "keys") => "mapKeys",
            (false, "values") => "mapValues",
            (false, "entries") => "mapEntries",
            _ => {
                return Err(self.source.locate(
                    unsupported(&format!("{}.prototype.{name}", kind.name())),
                    span,
                ));
            }
        };
        if name == "forEach" {
            self.check_for_each(call)?;
        }
        let storage = self.storage(receiver);
        // `add` and `set` return the collection itself.
        if matches!(name, "add" | "set") {
            let ty = Type::Collection(kind, id);
            let slot = self.hidden(storage.ty);
            let storage_ty = storage.ty;
            let marker = self.hidden(Type::Number);
            let output = self.hidden(ty);
            let call = self.collection_call(
                function,
                id,
                vec![local(span, slot, storage_ty)],
                &call.args,
                span,
            )?;
            let result = self.wrap(local(span, slot, storage_ty), id);
            return Ok(Expr {
                span,
                ty,
                kind: E::ValueBlock {
                    output,
                    marker,
                    roots: vec![slot, output],
                    body: vec![
                        S::Init(slot, storage),
                        S::Expr(call),
                        S::Init(output, result),
                    ],
                },
            });
        }
        self.collection_call(function, id, vec![storage], &call.args, span)
    }
    /// `forEach(callback)`: the callback takes at most `(value, key)`.
    fn check_for_each(&self, call: &ast::CallExpr) -> Result<()> {
        let parameters = match call.args.first().map(|arg| &*arg.expr) {
            Some(ast::Expr::Arrow(arrow)) => arrow.params.len(),
            Some(ast::Expr::Fn(function)) => function.function.params.len(),
            _ => 0,
        };
        if call.args.len() > 1 {
            return Err(self
                .source
                .locate(unsupported("forEach thisArg argument"), call.span));
        }
        if parameters > 2 {
            return Err(self.source.locate(
                unsupported("forEach callback's third (collection) parameter"),
                call.span,
            ));
        }
        Ok(())
    }
    /// `collection.size`, `bytes.length` and `bytes[i]` reads.
    pub(super) fn collection_member(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
    ) -> Result<Expr> {
        let Type::Collection(kind, id) = value.ty else {
            return Err(invalid("missing collection type"));
        };
        let span = member.span;
        let property = match &member.prop {
            ast::MemberProp::Ident(name) => Some(name.sym.as_ref()),
            _ => None,
        };
        match (kind, property) {
            (K::Set, Some("size")) => {
                let storage = self.storage(value);
                self.collection_call("setSize", id, vec![storage], &[], span)
            }
            (K::Map, Some("size")) => {
                let storage = self.storage(value);
                self.collection_call("mapSize", id, vec![storage], &[], span)
            }
            (K::Uint8Array, Some("length" | "byteLength")) => Ok(Expr {
                span,
                ty: self.types.number(),
                kind: E::Length(Box::new(self.storage(value))),
            }),
            (K::Uint8Array, Some("BYTES_PER_ELEMENT" | "byteOffset")) => Ok(Expr {
                span,
                ty: self.types.number(),
                kind: E::Sequence(
                    Box::new(value),
                    Box::new(Expr {
                        span,
                        ty: Type::Float64,
                        kind: E::Float64(if property == Some("byteOffset") {
                            BYTE_OFFSET
                        } else {
                            BYTES_PER_ELEMENT
                        }),
                    }),
                ),
            }),
            (K::Uint8Array, None) => {
                let storage = self.storage(value);
                self.array_member(member, storage, true)
            }
            _ => Err(self.source.locate(
                unsupported(&format!(
                    "{} property {}",
                    kind.name(),
                    property.unwrap_or("[computed]")
                )),
                span,
            )),
        }
    }
    /// `for (… of collection)`: a generator over the live entries (`Map`:
    /// `[key, value]` tuples), or the byte array itself.
    pub(super) fn collection_iteration(&mut self, value: Expr) -> Result<(Expr, bool)> {
        let Type::Collection(kind, id) = value.ty else {
            return Ok((value, false));
        };
        let span = value.span;
        let storage = self.storage(value);
        Ok(match kind {
            K::Uint8Array => (storage, true),
            K::Set => (
                self.collection_call("setValues", id, vec![storage], &[], span)?,
                false,
            ),
            K::Map => (
                self.collection_call("mapEntries", id, vec![storage], &[], span)?,
                false,
            ),
        })
    }
    /// `[...collection]`, the one admitted array literal spread (v37): a
    /// fresh array of the collection's iterated values; `None` otherwise.
    pub(super) fn spread_copy(&mut self, array: &ast::ArrayLit) -> Result<Option<Expr>> {
        let [Some(entry)] = array.elems.as_slice() else {
            return Ok(None);
        };
        if entry.spread.is_none() {
            return Ok(None);
        }
        if self.collection_iterator(&entry.expr) {
            let value = self.expr(&entry.expr)?;
            return self.drain(value).map(Some);
        }
        if !self.collection_receiver(&entry.expr, 0) {
            return Ok(None);
        }
        let value = self.expr(&entry.expr)?;
        self.collection_array(value).map(Some)
    }
    /// Whether `expression` is `collection.keys()`, `.values()` or `.entries()`.
    pub(super) fn collection_iterator(&self, expression: &ast::Expr) -> bool {
        matches!(stmt::unparen(expression), ast::Expr::Call(call)
            if matches!(&call.callee, ast::Callee::Expr(callee)
                if matches!(stmt::unparen(callee), ast::Expr::Member(member)
                    if matches!(&member.prop, ast::MemberProp::Ident(id)
                        if matches!(id.sym.as_ref(), "keys" | "values" | "entries"))
                    && self.collection_receiver(&member.obj, 0))))
    }
    /// A fresh array of the values a collection iterator yields.
    pub(super) fn drain(&mut self, iterator: Expr) -> Result<Expr> {
        let Type::Generator(id) = iterator.ty else {
            return Err(invalid("missing collection iterator type"));
        };
        let yielded = self.types.generators[id as usize].yielded;
        let span = iterator.span;
        self.engine_apply((&COLLECTIONS, "drain", &[yielded]), vec![iterator], span)
    }
    /// `[...collection]` and `Array.from(collection)`: a fresh array of the
    /// iterated values.
    pub(super) fn collection_array(&mut self, value: Expr) -> Result<Expr> {
        let Type::Collection(kind, id) = value.ty else {
            return Err(invalid("missing collection type"));
        };
        let span = value.span;
        let storage = self.storage(value);
        let function = match kind {
            K::Set => "setToArray",
            K::Map => "mapToArray",
            K::Uint8Array => "bytesToArray",
        };
        self.collection_call(function, id, vec![storage], &[], span)
    }
}

/// The array type a collection constructor accepts for collection `id`.
fn element_hint_for(lower: &mut Lower<'_>, id: u32) -> Result<Type> {
    let collection = lower.collection(id);
    match collection.kind {
        K::Map => {
            let pair = tuples::intern(
                vec![collection.key, collection.value],
                tuples::PAIR,
                false,
                lower.types,
            )?;
            arrays::intern(pair, lower.types)
        }
        _ => arrays::intern(collection.key, lower.types),
    }
}
