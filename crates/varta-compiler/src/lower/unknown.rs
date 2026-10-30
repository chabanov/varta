//! `unknown` (v34 `unknown_type`) and checked type assertions (v34
//! `checked_assertions`). Every value enters `unknown` as a reference-union
//! cell, exactly like widening into a closed reference union; only member tag,
//! class descriptor and array element-kind tests extract a value again, so no
//! assertion or guard ever reinterprets bits.
use super::*;
use references::Guard;
use swc_common::Span;
use varta_common::value::ErrorClass;

/// Longest quoted assertion target in a diagnostic or `TypeError` message.
const MAX_QUOTED_TYPE: usize = 64;

fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn boolean(span: Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn test(span: Span, kind: E) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind,
    }
}
fn and(span: Span, a: Expr, b: Expr) -> Expr {
    test(
        span,
        E::Conditional(Box::new(a), Box::new(b), Box::new(boolean(span, false))),
    )
}
fn or(span: Span, a: Expr, b: Expr) -> Expr {
    test(
        span,
        E::Conditional(Box::new(a), Box::new(boolean(span, true)), Box::new(b)),
    )
}

/// Whether `unknown` can hold a value of `ty`: every type whose values carry a
/// reference-cell tag under the profile's value layout, including value15
/// dictionaries and wide records. Function values have no cell tag in the
/// value13/value15 layouts; generators, promises and result pairs have none at all.
pub(super) fn admissible(ty: Type) -> bool {
    matches!(
        ty.erased(),
        Type::Float64
            | Type::Boolean
            | Type::String
            | Type::Null
            | Type::Undefined
            | Type::BigInt
            | Type::Error
            | Type::NativeError(_)
            | Type::Record(_)
            | Type::Dictionary(_)
            | Type::Class(_)
            | Type::Array(_)
            | Type::Union(_)
            | Type::ReferenceUnion(_)
            | Type::Unknown
            | Type::Object(_)
    )
}

/// Widen an admitted value into `unknown`: a closed union keeps its cell, any
/// other value is boxed into a fresh immutable cell (its identity is unchanged).
pub(super) fn widen(value: Expr, types: &ir::ValueTypes) -> Result<Expr> {
    let value = literals::erase(value, types);
    if value.ty == Type::Unknown {
        return Ok(value);
    }
    if collections::reaches(value.ty, types) {
        return Err(collections::opaque("conversion to unknown"));
    }
    // A tuple (v37) is an ordinary array to every dynamic reader.
    let value = match value.ty {
        Type::Tuple(id) => Expr {
            span: value.span,
            ty: types.tuples[id as usize].array,
            kind: E::Reinterpret(Box::new(value)),
        },
        _ => value,
    };
    if matches!(value.ty, Type::Promise(_)) && !types.profile.promise_values() {
        return Err(type_error(
            "promise value is not assignable to unknown; promises carry no value tag, \
             so await the promise or keep it typed as Promise<T>",
        ));
    }
    // Value16 promises (v35) are tagged cells; the check above rejects them
    // for earlier profiles.
    // Value17 function values (v36 `function_fields`) are tagged cells too.
    if !admissible(value.ty)
        && !matches!(value.ty, Type::Promise(_))
        && !(types.profile.function_fields() && matches!(value.ty, Type::Function(_)))
    {
        return Err(type_error(format!(
            "{} value is not assignable to unknown; unknown holds primitives, BigInt, \
             records, dictionaries, arrays and Error or class instances",
            value.ty
        )));
    }
    if classes::reaches_private(value.ty, types) {
        return Err(type_error(format!(
            "{} value is not assignable to unknown; it may reach a class instance with \
             #private fields, which host readers would see as properties",
            value.ty
        )));
    }
    if value.ty.is_union() {
        return Ok(Expr {
            ty: Type::Unknown,
            ..value
        });
    }
    Ok(Expr {
        span: value.span,
        ty: Type::Unknown,
        kind: E::BoxUnion(Box::new(value)),
    })
}

/// Members a guard proves for an `unknown` binding. Only positive tag-exact
/// guards narrow; `Array.isArray` and truthiness prove no representable type,
/// negated guards keep `unknown`, and `typeof x === "object"` proves only the
/// v35 `object | null` fact (see `lower::object_narrowing`).
pub(super) fn refine(guard: Guard, positive: bool, types: &ir::ValueTypes) -> Option<Vec<Type>> {
    match guard {
        Guard::Exact(ty) if positive => Some(vec![ty]),
        Guard::Literal(single) if positive => Some(vec![single]),
        // `typeof x === "object"` (v35 `unknown_json`): TypeScript's
        // `object | null`, a fact about the cell, not a member type.
        Guard::Object if positive && types.profile.unknown_json() => Some(vec![Type::Object(true)]),
        _ => None,
    }
}

/// What a guard proves for a binding known to hold `object` or, when
/// `nullable`, `object | null` (v35 `unknown_json`). Object values are always
/// truthy and never nullish; `Array.isArray` and `instanceof` keep the fact.
pub(super) fn refine_object(guard: Guard, positive: bool, nullable: bool) -> Option<Vec<Type>> {
    let non_null = match guard {
        Guard::Exact(Type::Null) => !positive,
        Guard::Nullish => !positive,
        Guard::Truth(truthy) => truthy == positive,
        _ => return None,
    };
    match (non_null, nullable) {
        (true, _) => Some(vec![Type::Object(false)]),
        (false, true) => Some(vec![Type::Null]),
        (false, false) => None,
    }
}

/// What `x instanceof Family` proves for `x: unknown`: the native errors of
/// the family and, when the program declares Error classes, the family root.
pub(super) fn error_family(family: ErrorClass, types: &ir::ValueTypes) -> Vec<Type> {
    let guard = Guard::Error(family);
    errors::classes(types.profile)
        .map(Type::from_error_class)
        .filter(|ty| guard.matches(*ty))
        .chain(types.classes.error_root(family).map(Type::Class))
        .collect()
}

/// The element kind that proves a vector holds exactly `element` values in
/// constant time: only primitive element types have a kind of their own.
fn primitive_kind(element: Type) -> Option<varta_common::value::ValueKind> {
    match element {
        Type::Float64
        | Type::Boolean
        | Type::String
        | Type::Null
        | Type::Undefined
        | Type::BigInt => element.value_kind(),
        _ => None,
    }
}

/// Whether a value of `source` may be used as `target` without a runtime test.
pub(super) fn upcast(source: Type, target: Type, types: &ir::ValueTypes) -> bool {
    if source == target || dictionaries::converts(target, source, types) {
        return true;
    }
    match target {
        Type::Unknown => {
            admissible(source)
                || types.profile.function_fields() && matches!(source, Type::Function(_))
        }
        Type::Union(_) | Type::ReferenceUnion(_) => {
            let target_members = unions::members(target, types);
            unions::members(source, types).iter().all(|member| {
                literals::member_of(*member, &target_members, types)
                    || matches!(*member, Type::Class(class) if target_members.iter().any(
                        |t| matches!(*t, Type::Class(base) if types.classes.descends(class, base))))
            })
        }
        Type::Class(base) => {
            matches!(source, Type::Class(class) if types.classes.descends(class, base))
        }
        _ => literals::assignable(target, source, types),
    }
}

/// Whether a cell of `source` that holds a member of `target` can be told
/// apart by its tag alone: the target is itself an array, dictionary, record or
/// class member of the closed source union (v35), whose member tags are
/// distinct, so the tag proves the exact member type.
fn member_tag(source: Type, target: Type, types: &ir::ValueTypes) -> bool {
    types.profile.mixed_index_records()
        && source.is_union()
        && matches!(
            target,
            Type::Array(_) | Type::Dictionary(_) | Type::Record(_) | Type::Class(_)
        )
        && unions::members(source, types).contains(&target)
}

/// Whether a dictionary is checkable by the exact descriptor of its storage
/// record (v35 `unknown_json`): the storage shape is interned by the value
/// type, so the descriptor proves the value type exactly, whatever it is.
fn storage_descriptor(types: &ir::ValueTypes) -> bool {
    types.profile.unknown_json()
}

/// Whether `cell_test` can prove that a cell of `source` holds a `target`
/// (the checks of a tested assertion or of a declared mixed-record read).
pub(super) fn checkable(source: Type, target: Type, types: &ir::ValueTypes) -> bool {
    checkable_visit(
        source,
        target,
        types,
        &mut std::collections::BTreeSet::new(),
    )
}
fn checkable_visit(
    source: Type,
    target: Type,
    types: &ir::ValueTypes,
    seen: &mut std::collections::BTreeSet<Type>,
) -> bool {
    if upcast(source, target, types) || member_tag(source, target, types) {
        return true;
    }
    if !seen.insert(target) {
        return true;
    }
    match target {
        Type::Unknown => true,
        Type::Union(_) | Type::ReferenceUnion(_) => unions::members(target, types)
            .into_iter()
            .all(|member| checkable_visit(source, member, types, seen)),
        Type::Float64
        | Type::Boolean
        | Type::String
        | Type::Null
        | Type::Undefined
        | Type::BigInt
        | Type::Error
        | Type::NativeError(_)
        | Type::Literal(..)
        | Type::Class(_) => true,
        Type::Record(shape) => types.records[shape as usize].class.is_none(),
        Type::Array(id) => primitive_kind(types.arrays[id as usize]).is_some(),
        Type::Dictionary(id) => {
            storage_descriptor(types)
                || primitive_kind(types.dictionaries[id as usize].value).is_some()
        }
        _ => false,
    }
}

/// TypeScript comparability of two non-union members, restricted to what a
/// checked assertion can test: equal physical types, literal sets of a base,
/// related classes, and native errors with each other or Error classes.
fn comparable(a: Type, b: Type, types: &ir::ValueTypes) -> bool {
    let classes = &types.classes;
    let error_class = |ty: Type| matches!(ty, Type::Class(id) if classes.is_error(id));
    a == Type::Unknown
        || b == Type::Unknown
        || a.erased() == b.erased()
        || matches!((a, b), (Type::Class(x), Type::Class(y)) if classes.descends(x, y) || classes.descends(y, x))
        || (a.is_error() || error_class(a)) && (b.is_error() || error_class(b))
}

impl Lower<'_> {
    /// The quoted source text of an assertion target.
    fn quoted(&self, span: Span, fallback: Type) -> String {
        let text = self
            .source
            .text(span)
            .unwrap_or_else(|| fallback.to_string());
        if text.chars().count() > MAX_QUOTED_TYPE {
            let short: String = text.chars().take(MAX_QUOTED_TYPE).collect();
            format!("{short}…")
        } else {
            text
        }
    }
    /// `x as T` (v34 `checked_assertions`): free when `x` already is a `T`
    /// (upcast), otherwise a runtime test that throws `TypeError` on mismatch.
    pub(super) fn type_assertion(&mut self, assertion: &ast::TsAsExpr) -> Result<Expr> {
        let annotation = &*assertion.type_ann;
        let mut target = keyword_type(annotation, self.types)
            .map_err(|e| self.source.locate(e, annotation.span()))?;
        // `x as Error` (or a family) also accepts Error-class instances, as
        // `instanceof` does; the result is the narrowed family union.
        if let ast::TsType::TsTypeRef(reference) = annotation
            && reference.type_params.is_none()
            && let ast::TsEntityName::Ident(id) = &reference.type_name
            && let Some(family) = ErrorClass::from_name(id.sym.as_ref())
            && !named::declared(self.types, id.sym.as_ref())
            && let Some(root) = self.types.classes.error_root(family)
        {
            let mut members = unions::members(target, self.types);
            members.push(Type::Class(root));
            target = unions::intern(members, self.types)?;
        }
        let name = self.quoted(annotation.span(), target);
        let operand = stmt::unparen(&assertion.expr);
        if let Some(parsed) = self.asserted_parse_json(operand, target, assertion.span)? {
            return Ok(parsed);
        }
        // TypeScript types an object or array literal operand contextually.
        let value = if matches!(operand, ast::Expr::Object(_) | ast::Expr::Array(_)) {
            self.expr_expected(&assertion.expr, Some(target))?
        } else {
            self.expr_preserving(&assertion.expr)?
        };
        let message = format!("type assertion to {name} failed");
        self.assert_value(value, target, &name, &message, assertion.span)
    }
    /// Convert `value` to `target`, testing the value at run time unless the
    /// conversion is an upcast. `message` is the `TypeError` message.
    pub(super) fn assert_value(
        &mut self,
        value: Expr,
        target: Type,
        name: &str,
        message: &str,
        span: Span,
    ) -> Result<Expr> {
        let source = value.ty;
        if upcast(source, target, self.types) {
            return unions::widen(value, Some(target), self.types);
        }
        if !admissible(source) || !admissible(target) {
            return Err(unsupported(&format!("type assertion to {name}")));
        }
        // V36: a downcast between dictionaries of one storage (a plain
        // `Record<string, T>` asserted to a mixed record of `T`) is
        // comparable in TypeScript and a free retag, because declared reads
        // are checked.
        if self.types.profile.checked_declared_fields()
            && matches!(target, Type::Dictionary(_))
            && dictionaries::converts(source, target, self.types)
            && !self
                .types
                .declared(target)
                .is_some_and(|members| members.object)
        {
            return Ok(Expr {
                ty: target,
                ..value
            });
        }
        let sources = unions::members(source, self.types);
        let targets = unions::members(target, self.types);
        if !targets
            .iter()
            .any(|t| sources.iter().any(|s| comparable(*s, *t, self.types)))
        {
            let aggregate = |ty: &Type| matches!(ty, Type::Record(_) | Type::Array(_));
            if sources.iter().any(aggregate) && targets.iter().any(aggregate) {
                return Err(unsupported(&format!(
                    "type assertion to {name} from a distinct record or array type; \
                     records and arrays are closed, so build the value at the asserted type"
                )));
            }
            return Err(type_error(format!(
                "type assertion from {source} to {name}: neither type sufficiently overlaps \
                 the other; assert through unknown first"
            )));
        }
        // V36 converts JSON-graph values to records, arrays and dictionaries.
        if declared_fields::applies(source, target, self.types) {
            return self.converted_value(value, target, message, span);
        }
        let cell_ty = if source.is_cell() {
            source
        } else {
            Type::Unknown
        };
        let cell = if source.is_cell() {
            value
        } else {
            widen(value, self.types)?
        };
        let saved = self.hidden(cell_ty);
        let check = self.cell_test(span, saved, cell_ty, target, name)?;
        let error = self.array_error(ErrorClass::TypeError, message, span)?;
        let converted = if target.is_cell() {
            local(span, saved, target)
        } else {
            Expr {
                span,
                ty: target,
                kind: E::UnboxUnion(Box::new(local(span, saved, cell_ty))),
            }
        };
        let output = self.hidden(target);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: target,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, cell),
                    S::If(
                        test(span, E::Not(Box::new(check))),
                        vec![S::Throw(error)],
                        vec![],
                    ),
                    S::Init(output, converted),
                ],
            },
        })
    }
    /// Whether the cell in `slot` (of cell type `source`) holds a `target`:
    /// member tags, exact literal values, the record descriptor of a structural
    /// shape, a class family's descriptors, or the element kind of a primitive
    /// array. Other targets are rejected rather than tested approximately.
    pub(super) fn cell_test(
        &mut self,
        span: Span,
        slot: u32,
        source: Type,
        target: Type,
        name: &str,
    ) -> Result<Expr> {
        let member = |ty: Type| {
            test(
                span,
                E::IsUnionMember(Box::new(local(span, slot, source)), ty),
            )
        };
        let unboxed = |ty: Type| Expr {
            span,
            ty,
            kind: E::UnboxUnion(Box::new(local(span, slot, source))),
        };
        if member_tag(source, target, self.types) {
            return Ok(member(target));
        }
        Ok(match target {
            Type::Unknown => boolean(span, true),
            Type::Union(_) | Type::ReferenceUnion(_) => {
                let mut check = boolean(span, false);
                for ty in unions::members(target, self.types).into_iter().rev() {
                    let next = self.cell_test(span, slot, source, ty, name)?;
                    check = or(span, next, check);
                }
                check
            }
            Type::Float64
            | Type::Boolean
            | Type::String
            | Type::Null
            | Type::Undefined
            | Type::BigInt
            | Type::Error
            | Type::NativeError(_) => member(target),
            Type::Literal(base, set) => {
                let base = base.erased();
                let mut values = boolean(span, false);
                for constant in literals::constants(set, span, self.types).into_iter().rev() {
                    let equal = test(
                        span,
                        E::Binary(
                            ast::BinaryOp::EqEqEq,
                            Box::new(unboxed(base)),
                            Box::new(constant),
                        ),
                    );
                    values = or(span, equal, values);
                }
                and(span, member(base), values)
            }
            Type::Record(shape) if self.types.records[shape as usize].class.is_none() => {
                let descriptor = test(
                    span,
                    E::IsClass {
                        value: Box::new(unboxed(target)),
                        shapes: vec![shape],
                        slot: self.hidden(Type::Number),
                    },
                );
                and(span, member(target), descriptor)
            }
            Type::Class(class) => {
                let descriptor = test(
                    span,
                    E::IsClass {
                        value: Box::new(unboxed(target)),
                        shapes: self.types.classes.family_shapes(class),
                        slot: self.hidden(Type::Number),
                    },
                );
                and(span, member(target), descriptor)
            }
            Type::Array(id) => {
                let kind = primitive_kind(self.types.arrays[id as usize]).ok_or_else(|| {
                    unsupported(&format!(
                        "type assertion to {name}: arrays are checked by element kind, so only \
                         arrays of number, string, boolean, bigint, null or undefined are checkable"
                    ))
                })?;
                and(
                    span,
                    member(target),
                    test(span, E::ArrayKind(Box::new(unboxed(target)), kind)),
                )
            }
            Type::Dictionary(id) if storage_descriptor(self.types) => {
                // The exact storage descriptor proves the value type, so a
                // dictionary of another value type (or a record or instance of
                // the same layout, which carries another tag) never passes.
                let storage = self.types.dictionaries[id as usize].storage;
                let descriptor = test(
                    span,
                    E::IsClass {
                        value: Box::new(unboxed(target)),
                        shapes: vec![storage],
                        slot: self.hidden(Type::Number),
                    },
                );
                and(span, member(target), descriptor)
            }
            Type::Dictionary(id) => {
                let dictionary = self.types.dictionaries[id as usize];
                let kind = primitive_kind(dictionary.value).ok_or_else(|| {
                    unsupported(&format!(
                        "type assertion to {name}: dictionaries are checked by the element kind \
                         of their value vector, so only dictionaries of number, string, boolean, \
                         bigint, null or undefined are checkable"
                    ))
                })?;
                let values = Expr {
                    span,
                    ty: dictionary.values,
                    kind: E::Field(Box::new(unboxed(target)), dictionaries::VALUES),
                };
                and(
                    span,
                    member(target),
                    test(span, E::ArrayKind(Box::new(values), kind)),
                )
            }
            _ => return Err(unsupported(&format!("type assertion to {name}"))),
        })
    }
    /// `x instanceof C` for `x: unknown`: the record tag and class descriptors.
    pub(super) fn unknown_instance_of(
        &mut self,
        value: Expr,
        target: Type,
        span: Span,
    ) -> Result<Expr> {
        let saved = self.hidden(Type::Unknown);
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let check = self.cell_test(span, saved, Type::Unknown, target, "a class")?;
        Ok(test(
            span,
            E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![S::Init(saved, value), S::Init(output, check)],
            },
        ))
    }
    /// `x!` (v34 `checked_assertions`): free when `x` cannot be nullish,
    /// otherwise a test that throws `TypeError` on `null` or `undefined`.
    pub(super) fn non_null_assertion(&mut self, assertion: &ast::TsNonNullExpr) -> Result<Expr> {
        let span = assertion.span;
        let value = self.expr_preserving(&assertion.expr)?;
        let source = value.ty;
        let nullish = |ty: &Type| matches!(ty, Type::Null | Type::Undefined);
        let members = unions::members(source, self.types);
        if source != Type::Unknown && !members.iter().any(nullish) {
            return Ok(value);
        }
        let remaining: Vec<_> = members.into_iter().filter(|ty| !nullish(ty)).collect();
        if remaining.is_empty() {
            return Err(type_error(format!(
                "non-null assertion on a value that is always {source}"
            )));
        }
        let target = unions::intern(remaining, self.types)?;
        let saved = self.hidden(source);
        let error = self.array_error(
            ErrorClass::TypeError,
            "non-null assertion failed: value is null or undefined",
            span,
        )?;
        let converted = if target.is_cell() {
            local(span, saved, target)
        } else {
            Expr {
                span,
                ty: target,
                kind: E::UnboxUnion(Box::new(local(span, saved, source))),
            }
        };
        let output = self.hidden(target);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: target,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, value),
                    S::If(
                        test(span, E::IsNullish(Box::new(local(span, saved, source)))),
                        vec![S::Throw(error)],
                        vec![],
                    ),
                    S::Init(output, converted),
                ],
            },
        })
    }
    /// `throw x` for `x: unknown`: the exception channel carries primitives,
    /// BigInt and Error values, so other members throw a `TypeError` instead.
    /// A caught value always passes, so rethrowing preserves it exactly.
    pub(super) fn thrown_unknown(&mut self, value: Expr) -> Result<Expr> {
        let span = value.span;
        let target = self.exception_type()?;
        self.assert_value(
            value,
            target,
            "a thrown value",
            "thrown value is not a primitive, BigInt or Error value",
            span,
        )
    }
    /// `String(x)` for `x: unknown`: primitives convert as `String`, native
    /// errors and Error-class instances as `Error.prototype.toString`, other
    /// records as `[object Object]`. Arrays and records of a program with a
    /// user `toString` method would need user dispatch and throw `TypeError`.
    pub(super) fn unknown_string(&mut self, value: Expr, span: Span) -> Result<Expr> {
        let saved = self.hidden(Type::Unknown);
        let mut primitives = vec![
            Type::Float64,
            Type::Boolean,
            Type::String,
            Type::Null,
            Type::Undefined,
            Type::BigInt,
        ];
        primitives.retain(|ty| admissible(*ty));
        let primitive = unions::intern(primitives, self.types)?;
        let is_primitive = self.cell_test(span, saved, Type::Unknown, primitive, "a primitive")?;
        let text = self.primitive_convert(local(span, saved, primitive), &[], false, span)?;
        let natives = errors::annotation(ErrorClass::Error, self.types)?;
        let is_native = self.cell_test(span, saved, Type::Unknown, natives, "Error")?;
        let native = Expr {
            span,
            ty: Type::String,
            kind: E::ErrorText(Box::new(local(span, saved, natives))),
        };
        let unsupported_text = |this: &mut Self, what: &str| -> Result<S> {
            Ok(S::Throw(this.array_error(
                ErrorClass::TypeError,
                &format!("String conversion of {what} held by unknown is not supported"),
                span,
            )?))
        };
        let output = self.hidden(Type::String);
        let marker = self.hidden(Type::Number);
        let assign = |value: Expr| S::Init(output, value);
        // Objects: an Error-class instance, else a plain record or instance.
        let mut object = if self.types.classes.has_method("toString") {
            vec![unsupported_text(self, "an object with a toString method")?]
        } else {
            vec![assign(Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes("[object Object]")),
            })]
        };
        if let Some(root) = self.types.classes.error_root(ErrorClass::Error) {
            let class = Type::Class(root);
            let is_instance = self.cell_test(span, saved, Type::Unknown, class, "Error")?;
            let instance = Expr {
                span,
                ty: class,
                kind: E::UnboxUnion(Box::new(local(span, saved, Type::Unknown))),
            };
            let instance = self.class_error_text(instance, span)?;
            object = vec![S::If(is_instance, vec![assign(instance)], object)];
        }
        let is_array = test(
            span,
            E::Host(
                self.types.profile.host_operation(Host::ValueIsArray),
                vec![local(span, saved, Type::Unknown)],
            ),
        );
        let array = vec![unsupported_text(self, "an array")?];
        let body = vec![
            S::Init(saved, value),
            S::If(
                is_primitive,
                vec![assign(text)],
                vec![S::If(
                    is_native,
                    vec![assign(native)],
                    vec![S::If(is_array, array, object)],
                )],
            ),
        ];
        Ok(Expr {
            span,
            ty: Type::String,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body,
            },
        })
    }
}
