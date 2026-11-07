//! Integrity levels (v38 `integrity_levels`, K9 D2; ECMA-262 §7.3.15
//! SetIntegrityLevel, §7.3.16 TestIntegrityLevel, §20.1.2).
//!
//! `Object.preventExtensions`, `Object.seal` and `Object.freeze` add header
//! bits to the object block (`varta_common::value::HEAP_INTEGRITY_FLAGS`,
//! [`ir::Intrinsic::Integrity`]); a primitive is returned unchanged. A program
//! that loads them (the integrity prelude, `modules::engine::INTEGRITY`)
//! checks every write against the bits: [`guard`] rewrites each field,
//! element and length write, `push`, `pop` and deletion after lowering so
//! that the check runs after the right-hand side was evaluated and before
//! the store, and throws V8's `TypeError` through the prelude (modules are
//! strict code). Dictionary writes and deletions check in their shared
//! helpers, and `reverse`, `fill` and an argument-less `push` of a receiver
//! with a level run element by element in the prelude, so the first failing
//! step throws after the earlier ones took effect, as in the specification.
//! Private names, the private state of engine classes (a `Map`'s entries)
//! and the storage of a `Uint8Array` are no properties and stay writable.
//! A program that loads none of these members gets no check and keeps its
//! bytes.
//!
//! The helpers here ([`test`], [`set`], [`enabled`]) are the integrity
//! state that later object operations (Proxy invariants, K9 D6) build on.
use super::*;
use crate::modules::engine::{INTEGRITY, SYMBOL};
use crate::modules::symbol_keys;
use ast::BinaryOp as B;
use std::collections::BTreeSet;
use varta_common::value::{
    ErrorClass, HEAP_FROZEN_FLAG as FROZEN, HEAP_NON_EXTENSIBLE_FLAG as NON_EXTENSIBLE,
    HEAP_SEALED_FLAG as SEALED, ValueKind,
};

/// V8's description of a plain object or dictionary in a message.
const OBJECT: &str = "#<Object>";

/// An integrity level (§7.3.15): the header bits it sets.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Level {
    NonExtensible,
    Sealed,
    Frozen,
}
impl Level {
    pub(crate) fn bits(self) -> u32 {
        match self {
            Self::NonExtensible => NON_EXTENSIBLE,
            Self::Sealed => NON_EXTENSIBLE | SEALED,
            Self::Frozen => NON_EXTENSIBLE | SEALED | FROZEN,
        }
    }
}

/// What one of the six static members of `Object` does.
#[derive(Clone, Copy)]
enum Member {
    Set(Level),
    /// `isSealed`/`isFrozen`.
    Test(Level),
    IsExtensible,
}
fn member(name: &str) -> Option<Member> {
    Some(match name {
        "Object.preventExtensions" => Member::Set(Level::NonExtensible),
        "Object.seal" => Member::Set(Level::Sealed),
        "Object.freeze" => Member::Set(Level::Frozen),
        "Object.isSealed" => Member::Test(Level::Sealed),
        "Object.isFrozen" => Member::Test(Level::Frozen),
        "Object.isExtensible" => Member::IsExtensible,
        _ => return None,
    })
}
/// The callee of the field-definition check a derived constructor runs
/// right after `super(...)` ([`define_statement`]); the space keeps it out
/// of every source identifier.
pub(super) const DEFINE: &str = "#integrity define";
/// `#integrity define(this, "field")`: throws V8's `TypeError` when `this`
/// is not extensible, as defining the first own field would.
pub(super) fn define_statement(field: &str, span: swc_common::Span) -> ast::Stmt {
    ast::Stmt::Expr(ast::ExprStmt {
        span,
        expr: Box::new(ast::Expr::Call(ast::CallExpr {
            span,
            callee: ast::Callee::Expr(Box::new(ast::Expr::Ident(ast::Ident::new_no_ctxt(
                DEFINE.into(),
                span,
            )))),
            args: vec![
                ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(ast::Expr::This(ast::ThisExpr { span })),
                },
                ast::ExprOrSpread {
                    spread: None,
                    expr: Box::new(ast::Expr::Lit(ast::Lit::Str(field.into()))),
                },
            ],
            ..Default::default()
        })),
    })
}
/// Whether `name` is one of the members this module lowers.
pub(super) fn is_member(name: &str) -> bool {
    member(name).is_some()
}

/// Whether the program checks its writes against integrity levels: the
/// profile admits them and the program loaded the integrity prelude.
pub(crate) fn enabled(types: &ir::ValueTypes) -> bool {
    types.profile.integrity_levels() && engine::module(&INTEGRITY, types).is_some()
}

/// Whether the object `value` has any of the header bits `mask`.
pub(crate) fn test(value: Expr, mask: u32) -> Expr {
    Expr {
        span: value.span,
        ty: Type::Boolean,
        kind: E::Intrinsic(
            ir::Intrinsic::Integrity(ir::IntegrityOp::Test(mask)),
            vec![value],
        ),
    }
}
/// `value` after adding the header bits of `level` to it.
pub(crate) fn set(value: Expr, level: Level) -> Expr {
    Expr {
        span: value.span,
        ty: value.ty,
        kind: E::Intrinsic(
            ir::Intrinsic::Integrity(ir::IntegrityOp::Set(level.bits())),
            vec![value],
        ),
    }
}

/// The statement that gives `target` the integrity level of `source` (no
/// statement without integrity checks).
pub(super) fn inherit(source: Expr, target: Expr, types: &ir::ValueTypes) -> S {
    if !enabled(types) {
        return S::Block(vec![]);
    }
    let mut result = vec![];
    for level in [Level::NonExtensible, Level::Sealed, Level::Frozen] {
        let bit = match level {
            Level::NonExtensible => NON_EXTENSIBLE,
            Level::Sealed => SEALED,
            Level::Frozen => FROZEN,
        };
        result = vec![S::If(
            test(source.clone(), bit),
            vec![S::Expr(set(target.clone(), level))],
            result,
        )];
    }
    S::Block(result)
}

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn binary(op: B, left: Expr, right: Expr) -> Expr {
    Expr {
        span: left.span,
        ty: Type::Boolean,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
fn arithmetic(op: B, left: Expr, right: Expr) -> Expr {
    Expr {
        span: left.span,
        ty: Type::Float64,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
fn not(value: Expr) -> Expr {
    Expr {
        span: value.span,
        ty: Type::Boolean,
        kind: E::Not(Box::new(value)),
    }
}
fn length(array: Expr) -> Expr {
    Expr {
        span: array.span,
        ty: Type::Float64,
        kind: E::Length(Box::new(array)),
    }
}
fn has_element(array: Expr, index: Expr) -> Expr {
    Expr {
        span: array.span,
        ty: Type::Boolean,
        kind: E::ArrayHasOwn(Box::new(array), Box::new(index)),
    }
}
fn has_field(record: Expr, slot: u32) -> Expr {
    Expr {
        span: record.span,
        ty: Type::Boolean,
        kind: E::HasOwn(Box::new(record), Some(slot)),
    }
}
fn text(span: swc_common::Span, value: &str, types: &ir::ValueTypes) -> Expr {
    Expr {
        span,
        ty: Type::String,
        kind: E::String(types.profile.string_bytes(value)),
    }
}
fn any(mut tests: Vec<Expr>, span: swc_common::Span) -> Expr {
    let Some(mut result) = tests.pop() else {
        return boolean(span, false);
    };
    while let Some(test) = tests.pop() {
        result = binary(B::LogicalOr, test, result);
    }
    result
}
fn all(mut tests: Vec<Expr>, span: swc_common::Span) -> Expr {
    let Some(mut result) = tests.pop() else {
        return boolean(span, true);
    };
    while let Some(test) = tests.pop() {
        result = binary(B::LogicalAnd, test, result);
    }
    result
}

/// How an object's properties answer the integrity predicates.
enum Target {
    /// Not an object: frozen, sealed and not extensible.
    Primitive,
    /// A record or class instance.
    Record,
    Array,
    Dictionary,
    /// An object without own properties: an engine collection or a promise.
    Opaque,
    /// An `Error` object: its own `stack` accessor and, when given, its own
    /// `message` data property.
    Error,
    /// A union or `unknown` cell.
    Cell,
}

/// A materialized getter result is a cache, not an own data property.
/// User writes have already become setter calls (or the getter-only error)
/// during subtyping resolution; direct writes to this slot refresh the cache.
fn accessor_cache(shape: &ir::RecordShape, name: &str) -> bool {
    shape.fields.iter().any(|field| {
        object_accessors::accessor_field(&field.name).is_some_and(|(property, _)| property == name)
    })
}

/// The fields of a record shape that are own properties: string- and
/// symbol-keyed fields, with their slots and whether they are data
/// properties; an accessor property is listed once (by its getter or, when
/// it has none, its setter).
fn properties(shape: &ir::RecordShape) -> Vec<(u32, bool, bool)> {
    let mut result = Vec::new();
    let names: BTreeSet<&str> = shape
        .fields
        .iter()
        .filter_map(|field| object_accessors::accessor_field(&field.name))
        .filter(|(_, getter)| *getter)
        .map(|(name, _)| name)
        .collect();
    for (slot, field) in shape.fields.iter().enumerate() {
        if let Some((name, getter)) = object_accessors::accessor_field(&field.name) {
            if getter || !names.contains(name) {
                // (slot, optional, data)
                result.push((slot as u32, field.optional, false));
            }
            continue;
        }
        if accessor_cache(shape, &field.name)
            || classes::is_private_name(&field.name) && !symbol_keys::is_symbol_key(&field.name)
        {
            continue;
        }
        result.push((slot as u32, field.optional, true));
    }
    result
}

impl Lower<'_> {
    /// `Object.freeze(o)`, `Object.seal(o)`, `Object.preventExtensions(o)`,
    /// `Object.isFrozen(o)`, `Object.isSealed(o)` and `Object.isExtensible(o)`.
    pub(super) fn integrity_static(&mut self, call: &ast::CallExpr, name: &str) -> Result<Expr> {
        let span = call.span;
        let member = member(name).ok_or_else(|| invalid("missing integrity member"))?;
        let [argument] = call.args.as_slice() else {
            return Err(self
                .source
                .locate(invalid(format!("{name} expects one argument")), span));
        };
        if argument.spread.is_some() {
            return Err(self
                .source
                .locate(unsupported(&format!("spread argument to {name}")), span));
        }
        let value = self.expr(&argument.expr)?;
        let target = self.integrity_target(value.ty, name, value.span)?;
        match member {
            Member::Set(level) => self.integrity_set(value, target, level, span),
            Member::IsExtensible => self.integrity_extensible(value, target, span),
            Member::Test(level) => self.integrity_level(value, target, level, name, span),
        }
    }
    /// The check of [`define_statement`].
    pub(super) fn integrity_define(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let span = call.span;
        let [instance, field] = call.args.as_slice() else {
            return Err(invalid("integrity define expects an instance and a field"));
        };
        let instance = self.expr(&instance.expr)?;
        let field = self.expr(&field.expr)?;
        let extensible = not(test(instance, NON_EXTENSIBLE));
        self.engine_apply(
            (&INTEGRITY, "integrityDefined", &[]),
            vec![extensible, field],
            span,
        )
    }
    fn integrity_target(&self, ty: Type, name: &str, span: swc_common::Span) -> Result<Target> {
        let refuse = |what: &str| {
            Err(self
                .source
                .locate(unsupported(&format!("{name} of {what}")), span))
        };
        Ok(match ty {
            Type::Number
            | Type::Float64
            | Type::String
            | Type::Boolean
            | Type::BigInt
            | Type::Symbol
            | Type::Undefined
            | Type::Null
            | Type::Literal(..)
            | Type::Union(_) => Target::Primitive,
            Type::Record(_) => Target::Record,
            Type::Class(class) if self.types.classes.is_error(class) => {
                return refuse(
                    "an Error subclass instance (its own stack, message and cause properties are not modelled)",
                );
            }
            Type::Class(_) => Target::Record,
            Type::Array(_) | Type::Tuple(_) => Target::Array,
            Type::Dictionary(_) => Target::Dictionary,
            Type::Collection(ir::CollectionKind::Uint8Array, _) => {
                return refuse(
                    "a Uint8Array (its elements stay configurable and freezing a non-empty one throws; not modelled)",
                );
            }
            Type::Collection(..) | Type::Promise(_) => Target::Opaque,
            Type::Error | Type::NativeError(_) => Target::Error,
            Type::Unknown | Type::ReferenceUnion(_) | Type::Object(_) => Target::Cell,
            Type::Function(_) => {
                return refuse("a function (functions have no own-property storage yet)");
            }
            Type::Generator(_) => {
                return refuse("a generator (its frame is not an integrity-level object)");
            }
            _ => return refuse(&format!("a value of type {ty}")),
        })
    }
    /// `value` in a rooted hidden local, then `body` of that local; the block's
    /// value is `result` of the local.
    fn integrity_block(
        &mut self,
        value: Expr,
        ty: Type,
        build: impl FnOnce(&mut Self, Expr) -> Result<(Vec<S>, Expr)>,
    ) -> Result<Expr> {
        let span = value.span;
        let source = self.hidden(value.ty);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let held = local(span, source, value.ty);
        let mut body = vec![S::Init(source, value)];
        let (statements, result) = build(self, held)?;
        body.extend(statements);
        body.push(S::Init(output, result));
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
    fn integrity_set(
        &mut self,
        value: Expr,
        target: Target,
        level: Level,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let ty = value.ty;
        match target {
            Target::Primitive => Ok(value),
            Target::Cell => {
                // A function or a `Uint8Array` held in a cell has no flaggable
                // own-property storage here: refuse it at run time.
                let refused = unions::members(ty, self.types)
                    .iter()
                    .any(|member| matches!(member, Type::Function(_) | Type::Collection(..)))
                    || ty == Type::Unknown
                    || matches!(ty, Type::Object(_));
                let error = self.array_error(
                    ErrorClass::TypeError,
                    "Object.freeze, seal and preventExtensions of a function or a Uint8Array are not supported (their own properties are not modelled)",
                    span,
                )?;
                self.integrity_block(value, ty, |_, held| {
                    let mut body = Vec::new();
                    if refused {
                        let kind = |kind| Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::ArrayKind(Box::new(held.clone()), kind),
                        };
                        body.push(S::If(
                            any(
                                vec![kind(ValueKind::Function), kind(ValueKind::Bytes)],
                                span,
                            ),
                            vec![S::Throw(error)],
                            vec![],
                        ));
                    }
                    Ok((body, set(held, level)))
                })
            }
            _ => Ok(set(value, level)),
        }
    }
    fn integrity_extensible(
        &mut self,
        value: Expr,
        target: Target,
        span: swc_common::Span,
    ) -> Result<Expr> {
        match target {
            Target::Primitive => Ok(Expr {
                span,
                ty: Type::Boolean,
                kind: E::Sequence(Box::new(value), Box::new(boolean(span, false))),
            }),
            Target::Cell => self.integrity_block(value, Type::Boolean, |_, held| {
                let kind = |kind| Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::ArrayKind(Box::new(held.clone()), kind),
                };
                let object = any(
                    [
                        ValueKind::Record,
                        ValueKind::Array,
                        ValueKind::Error,
                        ValueKind::Dictionary,
                        ValueKind::Promise,
                        ValueKind::Function,
                        ValueKind::Bytes,
                    ]
                    .into_iter()
                    .map(kind)
                    .collect(),
                    span,
                );
                let result = binary(
                    B::LogicalAnd,
                    object,
                    not(test(held.clone(), NON_EXTENSIBLE)),
                );
                Ok((vec![], result))
            }),
            _ => Ok(not(test(value, NON_EXTENSIBLE))),
        }
    }
    /// `Object.isSealed(value)` (`level` sealed) or `Object.isFrozen(value)`:
    /// not extensible, and the level's bit, or every own property already at
    /// the level (§7.3.16).
    fn integrity_level(
        &mut self,
        value: Expr,
        target: Target,
        level: Level,
        name: &str,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let frozen = level == Level::Frozen;
        let bit = if frozen { FROZEN } else { SEALED };
        match target {
            Target::Primitive => Ok(Expr {
                span,
                ty: Type::Boolean,
                kind: E::Sequence(Box::new(value), Box::new(boolean(span, true))),
            }),
            Target::Cell => Err(self.source.locate(
                unsupported(&format!(
                    "{name} of a value of type {} (the own properties of an object in a cell are not known statically; narrow it first)",
                    value.ty
                )),
                value.span,
            )),
            // Every engine collection and promise has no own property.
            Target::Opaque => Ok(test(value, NON_EXTENSIBLE)),
            // `length` is an own writable, non-configurable data property.
            Target::Array if frozen => Ok(test(value, FROZEN)),
            Target::Array => {
                let value = self.physical_array(value)?;
                let Type::Array(id) = value.ty else {
                    return Err(invalid("missing array type"));
                };
                let element = self.types.arrays[id as usize];
                self.integrity_block(value, Type::Boolean, |this, held| {
                    let elements = this.engine_apply(
                        (&INTEGRITY, "integrityHasElements", &[element]),
                        vec![held.clone()],
                        span,
                    )?;
                    let result = binary(
                        B::LogicalAnd,
                        test(held.clone(), NON_EXTENSIBLE),
                        binary(B::LogicalOr, test(held, SEALED), not(elements)),
                    );
                    Ok((vec![], result))
                })
            }
            Target::Dictionary => self.integrity_block(value, Type::Boolean, |this, held| {
                let keys = this.dictionary_enumeration(held.clone(), true)?;
                let empty = binary(B::EqEqEq, length(keys), number(span, 0.0));
                let result = binary(
                    B::LogicalAnd,
                    test(held.clone(), NON_EXTENSIBLE),
                    binary(B::LogicalOr, test(held, bit), empty),
                );
                Ok((vec![], result))
            }),
            // `stack` is an own configurable accessor, so only the bits
            // decide `isSealed`; a sealed Error without an own `message` has
            // no writable data property.
            Target::Error => self.integrity_block(value, Type::Boolean, |_, held| {
                let own = Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::ErrorOwn(Box::new(held.clone())),
                };
                let result = if frozen {
                    binary(
                        B::LogicalAnd,
                        test(held.clone(), NON_EXTENSIBLE),
                        binary(
                            B::LogicalOr,
                            test(held.clone(), FROZEN),
                            binary(B::LogicalAnd, test(held, SEALED), not(own)),
                        ),
                    )
                } else {
                    test(held, SEALED)
                };
                Ok((vec![], result))
            }),
            // The own properties depend on the object's layout, which a view
            // or a class family leaves open until `subtyping::resolve`: the
            // write-check pass ([`guard`]) expands this test per layout.
            Target::Record => Ok(Expr {
                span,
                ty: Type::Boolean,
                kind: E::Intrinsic(
                    ir::Intrinsic::Integrity(ir::IntegrityOp::Level { frozen }),
                    vec![value],
                ),
            }),
        }
    }

    /// The integrity checks of a dictionary write of `key` into `receiver`
    /// (both rooted locals of the shared helper): replacing an entry fails
    /// on a frozen dictionary, adding one on a non-extensible one.
    pub(super) fn dictionary_write_checks(
        &mut self,
        receiver: Expr,
        key: Expr,
    ) -> Result<(Vec<S>, Vec<S>)> {
        if !enabled(self.types) {
            return Ok((vec![], vec![]));
        }
        let span = receiver.span;
        let object = text(span, OBJECT, self.types);
        let read_only = self.engine_apply(
            (&INTEGRITY, "integrityReadOnly", &[]),
            vec![key.clone(), object],
            span,
        )?;
        let add = self.engine_apply((&INTEGRITY, "integrityAdd", &[]), vec![key], span)?;
        Ok((
            vec![S::If(
                test(receiver.clone(), FROZEN),
                vec![S::Expr(read_only)],
                vec![],
            )],
            vec![S::If(
                test(receiver, NON_EXTENSIBLE),
                vec![S::Expr(add)],
                vec![],
            )],
        ))
    }
    /// The integrity check of deleting the present entry `key` of `receiver`.
    pub(super) fn dictionary_delete_checks(&mut self, receiver: Expr, key: Expr) -> Result<Vec<S>> {
        if !enabled(self.types) {
            return Ok(vec![]);
        }
        let span = receiver.span;
        let object = text(span, OBJECT, self.types);
        let delete = self.engine_apply(
            (&INTEGRITY, "integrityDelete", &[]),
            vec![key, object],
            span,
        )?;
        Ok(vec![S::If(
            test(receiver, SEALED),
            vec![S::Expr(delete)],
            vec![],
        )])
    }
    /// `array.reverse()`, `array.fill(...)` and `array.push()` of a program
    /// with integrity checks: a receiver with a level runs the prelude's
    /// element-by-element form (or, for `push()`, writes its `length`).
    pub(super) fn integrity_array_method(
        &mut self,
        base: Expr,
        element: Type,
        method: &str,
        operands: Vec<Expr>,
        native: impl FnOnce(Expr, Vec<Expr>) -> E,
        (ty, span): (Type, swc_common::Span),
    ) -> Result<Expr> {
        let array = base.ty;
        let source = self.hidden(array);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let mut body = vec![S::Init(source, base)];
        let mut roots = vec![source, output];
        let mut held = Vec::new();
        for operand in operands {
            let slot = self.hidden(operand.ty);
            held.push(local(span, slot, operand.ty));
            roots.push(slot);
            body.push(S::Init(slot, operand));
        }
        let receiver = local(span, source, array);
        let checked = match method {
            "push" => {
                let fail =
                    self.engine_apply((&INTEGRITY, "integrityLengthReadOnly", &[]), vec![], span)?;
                body.push(S::If(
                    test(receiver.clone(), FROZEN),
                    vec![S::Expr(fail)],
                    vec![],
                ));
                None
            }
            "reverse" => Some(self.engine_apply(
                (&INTEGRITY, "integrityReverse", &[element]),
                vec![receiver.clone()],
                span,
            )?),
            _ => {
                let mut args = vec![receiver.clone()];
                args.extend(held.iter().cloned());
                Some(self.engine_apply((&INTEGRITY, "integrityFill", &[element]), args, span)?)
            }
        };
        let native = Expr {
            span,
            ty,
            kind: native(receiver.clone(), held),
        };
        let result = match checked {
            Some(checked) => Expr {
                span,
                ty,
                kind: E::Conditional(
                    Box::new(test(receiver, NON_EXTENSIBLE)),
                    Box::new(checked),
                    Box::new(native),
                ),
            },
            None => native,
        };
        body.push(S::Init(output, result));
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

/// The prelude functions the write checks call.
struct Failures {
    read_only: usize,
    add: usize,
    delete: usize,
    index_read_only: usize,
    index_add: usize,
    index_delete: usize,
    length_read_only: usize,
}
impl Failures {
    fn resolve(types: &ir::ValueTypes) -> Result<Self> {
        let index = |name: &str| {
            types
                .engine
                .function(&INTEGRITY, name)
                .map(|signature| signature.index)
                .ok_or_else(|| {
                    CompilerError::Internal(format!("integrity function {name} is not loaded"))
                })
        };
        Ok(Self {
            read_only: index("integrityReadOnly")?,
            add: index("integrityAdd")?,
            delete: index("integrityDelete")?,
            index_read_only: index("integrityIndexReadOnly")?,
            index_add: index("integrityIndexAdd")?,
            index_delete: index("integrityIndexDelete")?,
            length_read_only: index("integrityLengthReadOnly")?,
        })
    }
}
fn fail(span: swc_common::Span, function: usize, args: Vec<Expr>) -> S {
    S::Expr(Expr {
        span,
        ty: Type::Void,
        kind: E::Call(function, args),
    })
}

/// Rewrite every write of the program to check the written object's
/// integrity level first, when the program loaded the integrity prelude.
pub(super) fn guard(program: &mut ir::Program) -> Result<()> {
    if !enabled(&program.types) {
        return Ok(());
    }
    let failures = Failures::resolve(&program.types)?;
    for function in &mut program.functions {
        let mut guard = Guard {
            types: &program.types,
            locals: &mut function.locals,
            failures: &failures,
            temporaries: BTreeSet::new(),
        };
        guard.body(&mut function.body);
        prune::statements(&mut function.body, &mut |expr| guard.expression(expr));
    }
    Ok(())
}

struct Guard<'a> {
    types: &'a ir::ValueTypes,
    locals: &'a mut Vec<Type>,
    failures: &'a Failures,
    /// The locals this pass introduced: a write through one is checked.
    temporaries: BTreeSet<u32>,
}
impl Guard<'_> {
    fn local(&mut self, ty: Type) -> u32 {
        let slot = self.locals.len() as u32;
        self.locals.push(ty);
        self.temporaries.insert(slot);
        slot
    }
    /// Whether `expr` is a local this pass introduced (its write is checked).
    fn checked(&self, expr: &Expr) -> bool {
        matches!(expr.kind, E::Local(slot) if self.temporaries.contains(&slot))
    }
    /// `value` in a fresh local, unless re-reading it after the operands
    /// that follow it (`later`) observes the same value: a local those
    /// operands do not mention, or a literal.
    fn hold(&mut self, value: Expr, later: &[&Expr], held: &mut Vec<(u32, Expr)>) -> Expr {
        let stable = match value.kind {
            _ if self.checked(&value) => true,
            E::Integer(_) | E::Float64(_) | E::String(_) => true,
            E::Local(slot) => !later.iter().any(|expr| {
                let mut counts = std::collections::BTreeMap::new();
                suspension::expression_counts(expr, &mut counts);
                counts.contains_key(&slot)
            }),
            _ => false,
        };
        if stable {
            return value;
        }
        self.temporary(value, held)
    }
    /// `value` in a fresh local, which marks the operation on it checked.
    fn temporary(&mut self, value: Expr, held: &mut Vec<(u32, Expr)>) -> Expr {
        let span = value.span;
        let ty = value.ty;
        let slot = self.local(ty);
        held.push((slot, value));
        local(span, slot, ty)
    }
    /// The statements that evaluate `held` into their locals, then `checks`
    /// and `write`, in a scope that clears the locals.
    fn sequence(held: Vec<(u32, Expr)>, checks: Vec<S>, write: S) -> S {
        let slots = held.iter().map(|(slot, _)| *slot).collect();
        let mut body: Vec<S> = held
            .into_iter()
            .map(|(slot, value)| S::Init(slot, value))
            .collect();
        body.extend(checks);
        body.push(write);
        S::Scope(slots, body)
    }

    fn body(&mut self, body: &mut Vec<S>) {
        for statement in std::mem::take(body) {
            let mut statement = statement;
            self.nested(&mut statement);
            body.push(self.statement(statement));
        }
    }
    fn nested(&mut self, statement: &mut S) {
        match statement {
            S::Block(body) | S::Scope(_, body) | S::Label { body, .. } => self.body(body),
            S::If(_, yes, no) => {
                self.body(yes);
                self.body(no);
            }
            S::While(_, body) | S::DoWhile(_, body) => self.body(body),
            S::For(init, _, update, body) => {
                self.body(init);
                self.body(update);
                self.body(body);
            }
            S::Switch { cases, .. } => {
                for (_, body) in cases {
                    self.body(body);
                }
            }
            S::TryCatch { body, handler, .. } => {
                self.body(body);
                self.body(handler);
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                self.body(body);
                self.body(finalizer);
            }
            _ => {}
        }
    }
    fn statement(&mut self, statement: S) -> S {
        match statement {
            S::SetField(object, slot, value) if !self.checked(&object) => {
                self.set_field(object, slot, value)
            }
            S::SetIndex {
                array,
                index,
                value,
                slots,
            } if !self.checked(&array) && self.element_array(array.ty) => {
                self.set_index(array, index, value, slots)
            }
            S::ArrayLength {
                array,
                length,
                error,
            } if !self.checked(&array) && self.element_array(array.ty) => {
                self.array_length(array, length, error)
            }
            statement => statement,
        }
    }
    /// Whether writes to an array of type `ty` check its level: every array
    /// but the storage of a `Uint8Array`, whose elements ignore extensibility.
    fn element_array(&self, ty: Type) -> bool {
        matches!(ty, Type::Array(id) if self.types.bytes != Some(id))
    }
    /// The record shape behind an object type, if its fields are properties.
    fn shape(&self, ty: Type) -> Option<u32> {
        match ty {
            Type::Record(shape) => Some(shape),
            Type::Class(class) => Some(self.types.classes.shape(class)),
            _ => None,
        }
    }
    /// A property key as V8 prints it: a string key itself, a symbol key as
    /// `Symbol(description)` of its symbol, read at run time.
    fn key_text(&mut self, key: &str, span: swc_common::Span) -> Expr {
        self.symbol_text(key, span)
            .unwrap_or_else(|| text(span, key, self.types))
    }
    fn symbol_text(&mut self, key: &str, span: swc_common::Span) -> Option<Expr> {
        if !symbol_keys::is_symbol_key(key) {
            return None;
        }
        let record = Type::Class(self.types.symbol?);
        let function = |name: &str| self.types.engine.function(&SYMBOL, name).map(|s| s.index);
        let symbol = match symbol_keys::well_known_name(key) {
            Some(name) => Expr {
                span,
                ty: record,
                kind: E::Call(
                    function("wellKnownSymbol")?,
                    vec![number(span, symbols::well_known(name)? as f64)],
                ),
            },
            None => Expr {
                span,
                ty: record,
                kind: E::ModuleConstant(*self.types.symbols.constants.get(key)?),
            },
        };
        Some(Expr {
            span,
            ty: Type::String,
            kind: E::Call(function("symbolToString")?, vec![symbol]),
        })
    }
    /// V8's description of the object `held` of type `ty` (`#<Name>` by the
    /// instance's class).
    fn description(&mut self, held: &Expr) -> Expr {
        let span = held.span;
        let class = match held.ty {
            Type::Class(class) => Some(class),
            Type::Record(shape) => self.types.records[shape as usize].class,
            _ => None,
        };
        let Some(class) = class else {
            return text(span, OBJECT, self.types);
        };
        let family = self.types.classes.family_names(class);
        let own = self.types.classes.shape(class);
        let name = |name: &str| text(span, &format!("#<{name}>"), self.types);
        let mut result = family
            .iter()
            .find(|(shape, _)| *shape == own)
            .map_or_else(|| text(span, OBJECT, self.types), |(_, own)| name(own));
        for (shape, class_name) in family.iter().filter(|(shape, _)| *shape != own) {
            let slot = self.local(Type::Number);
            let test = Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsClass {
                    value: Box::new(held.clone()),
                    shapes: vec![*shape],
                    slot,
                },
            };
            result = Expr {
                span,
                ty: Type::String,
                kind: E::Conditional(Box::new(test), Box::new(name(class_name)), Box::new(result)),
            };
        }
        result
    }
    fn set_field(&mut self, object: Expr, slot: u32, value: Expr) -> S {
        let Some(shape) = self.shape(object.ty) else {
            return S::SetField(object, slot, value);
        };
        let Some(field) = self.types.records[shape as usize].fields.get(slot as usize) else {
            return S::SetField(object, slot, value);
        };
        if accessor_cache(&self.types.records[shape as usize], &field.name)
            || classes::is_private_name(&field.name) && !symbol_keys::is_symbol_key(&field.name)
        {
            return S::SetField(object, slot, value);
        }
        let (key, optional) = (field.name.clone(), field.optional);
        let span = object.span;
        let mut held = Vec::new();
        let object = self.hold(object, &[&value], &mut held);
        let value = self.hold(value, &[], &mut held);
        let key = self.key_text(&key, span);
        let description = self.description(&object);
        let read_only = S::If(
            test(object.clone(), FROZEN),
            vec![fail(
                span,
                self.failures.read_only,
                vec![key.clone(), description],
            )],
            vec![],
        );
        let check = if optional {
            S::If(
                test(object.clone(), NON_EXTENSIBLE),
                vec![S::If(
                    has_field(object.clone(), slot),
                    vec![read_only],
                    vec![fail(span, self.failures.add, vec![key])],
                )],
                vec![],
            )
        } else {
            read_only
        };
        Self::sequence(held, vec![check], S::SetField(object, slot, value))
    }
    fn set_index(&mut self, array: Expr, index: Expr, value: Expr, slots: Option<(u32, u32)>) -> S {
        let span = array.span;
        let mut held = Vec::new();
        let array = self.hold(array, &[&index, &value], &mut held);
        let index = self.hold(index, &[&value], &mut held);
        let value = self.hold(value, &[], &mut held);
        let check = S::If(
            test(array.clone(), NON_EXTENSIBLE),
            vec![S::If(
                has_element(array.clone(), index.clone()),
                vec![S::If(
                    test(array.clone(), FROZEN),
                    vec![fail(
                        span,
                        self.failures.index_read_only,
                        vec![index.clone()],
                    )],
                    vec![],
                )],
                vec![fail(span, self.failures.index_add, vec![index.clone()])],
            )],
            vec![],
        );
        Self::sequence(
            held,
            vec![check],
            S::SetIndex {
                array,
                index,
                value,
                slots,
            },
        )
    }
    /// `array.length = n` (§10.4.2.4 ArraySetLength): a frozen array's
    /// `length` is read-only; shrinking a sealed array stops at its last
    /// element at or above `n`, which is not deletable.
    fn array_length(&mut self, array: Expr, length_value: Expr, error: Expr) -> S {
        let span = array.span;
        let mut held = Vec::new();
        let array = self.hold(array, &[&length_value], &mut held);
        let target = self.hold(length_value, &[], &mut held);
        let last = self.local(Type::Float64);
        let at = || local(span, last, Type::Float64);
        let valid = binary(
            B::LogicalAnd,
            binary(B::GtEq, target.clone(), number(span, 0.0)),
            binary(
                B::EqEqEq,
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Math(ir::MathIntrinsic::Floor, vec![target.clone()]),
                },
                target.clone(),
            ),
        );
        let shrink = binary(
            B::LogicalAnd,
            test(array.clone(), SEALED),
            binary(
                B::LogicalAnd,
                valid,
                binary(B::Lt, target.clone(), length(array.clone())),
            ),
        );
        let stop = vec![
            S::Init(
                last,
                arithmetic(B::Sub, length(array.clone()), number(span, 1.0)),
            ),
            S::While(
                binary(
                    B::LogicalAnd,
                    binary(B::GtEq, at(), target.clone()),
                    not(has_element(array.clone(), at())),
                ),
                vec![S::Set(last, arithmetic(B::Sub, at(), number(span, 1.0)))],
            ),
            S::If(
                binary(B::GtEq, at(), target.clone()),
                vec![
                    S::ArrayLength {
                        array: array.clone(),
                        length: arithmetic(B::Add, at(), number(span, 1.0)),
                        error: error.clone(),
                    },
                    fail(span, self.failures.index_delete, vec![at()]),
                ],
                vec![],
            ),
        ];
        let check = S::If(
            test(array.clone(), NON_EXTENSIBLE),
            vec![
                S::If(
                    test(array.clone(), FROZEN),
                    vec![fail(span, self.failures.length_read_only, vec![])],
                    vec![],
                ),
                S::If(shrink, stop, vec![]),
            ],
            vec![],
        );
        Self::sequence(
            held,
            vec![check],
            S::ArrayLength {
                array,
                length: target,
                error,
            },
        )
    }

    /// Visit one expression of the prune walk (parents before children).
    fn expression(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            E::ValueBlock { body, .. } | E::Assignment { body, .. } => self.body(body),
            E::Intrinsic(ir::Intrinsic::Integrity(ir::IntegrityOp::Level { frozen }), _) => {
                let frozen = *frozen;
                let placeholder = Expr {
                    span: expr.span,
                    ty: expr.ty,
                    kind: E::Zero,
                };
                let original = std::mem::replace(expr, placeholder);
                if let E::Intrinsic(_, mut args) = original.kind
                    && let Some(value) = args.pop()
                {
                    *expr = self.level(value, frozen);
                }
            }
            E::ArrayPush(array, _) | E::ArrayPop(array)
                if !self.checked(array) && self.element_array(array.ty) =>
            {
                let placeholder = Expr {
                    span: expr.span,
                    ty: expr.ty,
                    kind: E::Zero,
                };
                let original = std::mem::replace(expr, placeholder);
                *expr = self.array_change(original);
            }
            E::ArrayDelete {
                array,
                index: Some(_),
                error: None,
            } if !self.checked(array) && self.element_array(array.ty) => {
                let placeholder = Expr {
                    span: expr.span,
                    ty: expr.ty,
                    kind: E::Zero,
                };
                let original = std::mem::replace(expr, placeholder);
                *expr = self.array_delete(original);
            }
            E::RecordDelete { object, .. }
                if !self.checked(object) && self.shape(object.ty).is_some() =>
            {
                let placeholder = Expr {
                    span: expr.span,
                    ty: expr.ty,
                    kind: E::Zero,
                };
                let original = std::mem::replace(expr, placeholder);
                *expr = self.record_delete(original);
            }
            _ => {}
        }
    }
    /// The layouts a value of record or class type `ty` may have: its own,
    /// those of the view's sources and of class families.
    fn layouts(&self, ty: Type) -> Vec<u32> {
        match ty {
            Type::Record(shape) => self.types.subtyping.shapes(shape, self.types),
            Type::Class(class) => self
                .types
                .classes
                .family_names(class)
                .into_iter()
                .map(|(shape, _)| shape)
                .collect(),
            _ => Vec::new(),
        }
    }
    /// `Object.isFrozen(value)` (`frozen`) or `Object.isSealed(value)` of a
    /// record or class instance (§7.3.16): not extensible, and the level's
    /// bit or no own property (for frozen: no own data property and sealed
    /// or no own property), by the object's layout.
    fn level(&mut self, value: Expr, frozen: bool) -> Expr {
        let span = value.span;
        let mut held = Vec::new();
        let object = self.temporary(value, &mut held);
        // (no own property, no own data property) of the object.
        let mut absent: Option<(Expr, Expr)> = None;
        for layout in self.layouts(object.ty).into_iter().rev() {
            let viewed = Expr {
                span,
                ty: Type::Record(layout),
                kind: E::Reinterpret(Box::new(object.clone())),
            };
            let properties = properties(&self.types.records[layout as usize]);
            let none = |data: bool| {
                let mut tests = Vec::new();
                for (slot, optional, is_data) in &properties {
                    if data && !is_data {
                        continue;
                    }
                    if !optional {
                        return boolean(span, false);
                    }
                    tests.push(not(has_field(viewed.clone(), *slot)));
                }
                all(tests, span)
            };
            let own = (none(false), none(true));
            absent = Some(match absent {
                None => own,
                Some((properties, data)) => {
                    let slot = self.local(Type::Number);
                    let test = Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::IsClass {
                            value: Box::new(object.clone()),
                            shapes: vec![layout],
                            slot,
                        },
                    };
                    let choose = |yes: Expr, no: Expr| Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Conditional(Box::new(test.clone()), Box::new(yes), Box::new(no)),
                    };
                    (choose(own.0, properties), choose(own.1, data))
                }
            });
        }
        let (none, data) = absent.unwrap_or_else(|| (boolean(span, true), boolean(span, true)));
        let sealed = binary(B::LogicalOr, test(object.clone(), SEALED), none);
        let level = if frozen {
            binary(
                B::LogicalOr,
                test(object.clone(), FROZEN),
                binary(B::LogicalAnd, data, sealed),
            )
        } else {
            sealed
        };
        let result = binary(B::LogicalAnd, test(object, NON_EXTENSIBLE), level);
        self.block(span, Type::Boolean, held, vec![], result)
    }
    /// A value block of `held` operands, `checks`, then the operation.
    fn block(
        &mut self,
        span: swc_common::Span,
        ty: Type,
        held: Vec<(u32, Expr)>,
        checks: Vec<S>,
        operation: Expr,
    ) -> Expr {
        let output = self.local(ty);
        let marker = self.local(Type::Number);
        let mut roots: Vec<u32> = held.iter().map(|(slot, _)| *slot).collect();
        roots.push(output);
        let mut body: Vec<S> = held
            .into_iter()
            .map(|(slot, value)| S::Init(slot, value))
            .collect();
        body.extend(checks);
        body.push(S::Init(output, operation));
        Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        }
    }
    /// `push(...)` (§23.1.3.23: each element is added, then `length` set) and
    /// `pop()` (§23.1.3.22: the last element deleted, then `length` set).
    fn array_change(&mut self, expr: Expr) -> Expr {
        let (span, ty) = (expr.span, expr.ty);
        let mut held = Vec::new();
        let (checks, operation) = match expr.kind {
            E::ArrayPush(array, items) => {
                let array = self.temporary(*array, &mut held);
                let items = self.hold(*items, &[], &mut held);
                let check = S::If(
                    test(array.clone(), NON_EXTENSIBLE),
                    vec![S::If(
                        binary(B::Gt, length(items.clone()), number(span, 0.0)),
                        vec![fail(
                            span,
                            self.failures.index_add,
                            vec![length(array.clone())],
                        )],
                        vec![S::If(
                            test(array.clone(), FROZEN),
                            vec![fail(span, self.failures.length_read_only, vec![])],
                            vec![],
                        )],
                    )],
                    vec![],
                );
                (vec![check], E::ArrayPush(Box::new(array), Box::new(items)))
            }
            E::ArrayPop(array) => {
                let array = self.temporary(*array, &mut held);
                let count = self.local(Type::Float64);
                let last =
                    || arithmetic(B::Sub, local(span, count, Type::Float64), number(span, 1.0));
                let read_only = S::If(
                    test(array.clone(), FROZEN),
                    vec![fail(span, self.failures.length_read_only, vec![])],
                    vec![],
                );
                let check = S::If(
                    test(array.clone(), NON_EXTENSIBLE),
                    vec![
                        S::Init(count, length(array.clone())),
                        S::If(
                            binary(
                                B::EqEqEq,
                                local(span, count, Type::Float64),
                                number(span, 0.0),
                            ),
                            vec![read_only.clone()],
                            vec![S::If(
                                has_element(array.clone(), last()),
                                vec![S::If(
                                    test(array.clone(), SEALED),
                                    vec![fail(span, self.failures.index_delete, vec![last()])],
                                    vec![],
                                )],
                                vec![read_only],
                            )],
                        ),
                    ],
                    vec![],
                );
                (vec![check], E::ArrayPop(Box::new(array)))
            }
            kind => (vec![], kind),
        };
        let operation = Expr {
            span,
            ty,
            kind: operation,
        };
        self.block(span, ty, held, checks, operation)
    }
    /// `delete array[i]` of a present element of a sealed array.
    fn array_delete(&mut self, expr: Expr) -> Expr {
        let (span, ty) = (expr.span, expr.ty);
        let E::ArrayDelete {
            array,
            index: Some(index),
            error,
        } = expr.kind
        else {
            return expr;
        };
        let mut held = Vec::new();
        let array = self.temporary(*array, &mut held);
        let index = self.hold(*index, &[], &mut held);
        let check = S::If(
            binary(
                B::LogicalAnd,
                test(array.clone(), SEALED),
                has_element(array.clone(), index.clone()),
            ),
            vec![fail(span, self.failures.index_delete, vec![index.clone()])],
            vec![],
        );
        let operation = Expr {
            span,
            ty,
            kind: E::ArrayDelete {
                array: Box::new(array),
                index: Some(Box::new(index)),
                error,
            },
        };
        self.block(span, ty, held, vec![check], operation)
    }
    /// `delete record.key` of a present property of a sealed object.
    fn record_delete(&mut self, expr: Expr) -> Expr {
        let (span, ty) = (expr.span, expr.ty);
        let E::RecordDelete {
            object,
            absent,
            slot,
            scan,
            write,
            count,
        } = expr.kind
        else {
            return expr;
        };
        let key = self
            .shape(object.ty)
            .and_then(|shape| self.types.records[shape as usize].fields.get(slot as usize))
            .map(|field| field.name.clone())
            .unwrap_or_default();
        let mut held = Vec::new();
        let object = self.temporary(*object, &mut held);
        let description = self.description(&object);
        let check = S::If(
            binary(
                B::LogicalAnd,
                test(object.clone(), SEALED),
                has_field(object.clone(), slot),
            ),
            vec![fail(
                span,
                self.failures.delete,
                vec![self.key_text(&key, span), description],
            )],
            vec![],
        );
        let operation = Expr {
            span,
            ty,
            kind: E::RecordDelete {
                object: Box::new(object),
                absent,
                slot,
                scan,
                write,
                count,
            },
        };
        self.block(span, ty, held, vec![check], operation)
    }
}
