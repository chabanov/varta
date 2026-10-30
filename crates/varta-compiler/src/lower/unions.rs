//! Closed primitive unions: exact widening, branch-local narrowing, no coercion.
use super::*;
use references::Guard;

pub(super) fn primitive(ty: Type) -> bool {
    matches!(
        ty,
        Type::Float64
            | Type::Boolean
            | Type::String
            | Type::Null
            | Type::Undefined
            | Type::Literal(..)
    )
}
pub(super) fn members(ty: Type, types: &ir::ValueTypes) -> Vec<Type> {
    // `never` (v37) has no member.
    if ty == Type::Never {
        return Vec::new();
    }
    if let Type::Union(id) | Type::ReferenceUnion(id) = ty {
        types.unions[id as usize].clone()
    } else {
        vec![ty]
    }
}
/// Whether the primitive cells of a conversion or comparison of values of
/// `sources` include the BigInt kind: always when the profile has BigInt
/// values, except under v38 (`linked_intrinsics`), where they do only when a
/// source may hold a BigInt, so a program that never converts or compares a
/// BigInt links no BigInt code behind the inline fast paths.
pub(super) fn cells_hold_bigint(sources: &[Type], types: &ir::ValueTypes) -> bool {
    types.profile.bigint_values()
        && (!types.profile.linked_intrinsics()
            || sources
                .iter()
                .any(|source| members(*source, types).contains(&Type::BigInt)))
}
/// The primitive kinds of the cells of a v38 (`linked_intrinsics`)
/// comparison of values of `sources`, in the order of the full cell union:
/// the kinds a source may hold (literals by their base kind), so the inline
/// fast path in front of the linked comparison is emitted (and links code)
/// only for kinds that occur. At least two, so the cell stays a union.
pub(super) fn comparison_cells(sources: &[Type], types: &ir::ValueTypes) -> Vec<Type> {
    const KINDS: [Type; 6] = [
        Type::Float64,
        Type::String,
        Type::Boolean,
        Type::Null,
        Type::Undefined,
        Type::BigInt,
    ];
    let held: Vec<Type> = sources.iter().flat_map(|s| members(*s, types)).collect();
    let mut kinds: Vec<Type> = KINDS
        .into_iter()
        .filter(|kind| {
            held.iter()
                .any(|member| literals::member_of(*member, &[*kind], types))
        })
        .collect();
    for filler in [Type::Undefined, Type::Null] {
        if kinds.len() < 2 && !kinds.contains(&filler) {
            kinds.push(filler);
        }
    }
    kinds
}
pub(super) fn intern(values: Vec<Type>, types: &mut ir::ValueTypes) -> Result<Type> {
    if !types.profile.unions() {
        return Err(unsupported("union types require checked-f64-utf16-v2"));
    }
    // A reserved recursive union (empty members) cannot be flattened into
    // another union before its declaration is resolved.
    if values
        .iter()
        .any(|ty| matches!(ty, Type::ReferenceUnion(id) if types.unions[*id as usize].is_empty()))
    {
        return Err(unsupported(
            "recursive type alias as a direct member of a union; refer to it through an array, record or dictionary",
        ));
    }
    let never = values.contains(&Type::Never);
    let mut values: Vec<_> = values
        .into_iter()
        .flat_map(|ty| members(ty, types))
        .collect();
    // `never | T` is `T`, and a union of `never` alone is `never` (v37).
    if never && values.is_empty() {
        return Ok(Type::Never);
    }
    if values.iter().any(|ty| matches!(ty, Type::Generator(_))) {
        return Err(generators::opaque("a union"));
    }
    if values.iter().any(|ty| matches!(ty, Type::Promise(_))) && !types.profile.promise_values() {
        return Err(promises::opaque("a union"));
    }
    // `unknown` absorbs every other member (v34); `widen` checks admission.
    if values.contains(&Type::Unknown) {
        return Ok(Type::Unknown);
    }
    // `object` facts (v35) join with `null` and with the dictionary views of
    // `in` guards; any other member leaves only `unknown`.
    let view = |ty: &Type| types.declared(*ty).is_some_and(|members| members.object);
    let views = {
        let mut views: Vec<_> = values.iter().copied().filter(view).collect();
        views.sort();
        views.dedup();
        views.len()
    };
    if values.iter().any(|ty| matches!(ty, Type::Object(_))) || views > 1 {
        let objects = values
            .iter()
            .all(|ty| matches!(ty, Type::Object(_) | Type::Null) || view(ty));
        let nullable = values
            .iter()
            .any(|ty| matches!(ty, Type::Object(true) | Type::Null));
        return Ok(if objects {
            Type::Object(nullable)
        } else {
            Type::Unknown
        });
    }
    if types.profile.error_subclasses() {
        values = classes::join_error_classes(values, types);
    }
    values.sort();
    values.dedup();
    let values = literals::normalize(values, types)?;
    if values.is_empty()
        || values.iter().any(|ty| {
            *ty == Type::BigInt && !types.profile.bigint_values()
                || !primitive(*ty)
                    && !(types.profile.reference_unions()
                        && matches!(ty, Type::Record(_) | Type::Array(_))
                        || types.profile.index_signatures() && matches!(ty, Type::Dictionary(_))
                        || types.profile.classes() && matches!(ty, Type::Class(_))
                        || types.profile.bigint_values() && *ty == Type::BigInt
                        || types.profile.promise_values() && matches!(ty, Type::Promise(_))
                        || types.profile.function_fields() && matches!(ty, Type::Function(_))
                        || types.profile.collections()
                            && matches!(ty, Type::Tuple(_) | Type::Collection(..))
                        || (types.profile.errors() && ty.is_error()))
        })
    {
        return Err(unsupported(
            "union members other than number, boolean, string, null or undefined",
        ));
    }
    let references = values.iter().any(|ty| {
        matches!(
            ty,
            Type::BigInt
                | Type::Record(_)
                | Type::Dictionary(_)
                | Type::Class(_)
                | Type::Array(_)
                | Type::Promise(_)
                | Type::Function(_)
                | Type::Error
                | Type::NativeError(_)
                | Type::Tuple(_)
                | Type::Collection(..)
        )
    });
    // Records and class instances share the record union tag; so do map and
    // set storage (v37). Tuples and byte arrays share the array tag. V37
    // (`language_core`) tells several records and classes apart by their
    // layout descriptors, and several arrays by distinct element kinds;
    // collections and tuples take part only as the single member of their
    // tag.
    let shared = types.profile.language_core();
    let byte_array = |ty: &&Type| matches!(ty, Type::Collection(ir::CollectionKind::Uint8Array, _));
    let record_tag: Vec<&Type> = values
        .iter()
        .filter(|ty| {
            matches!(ty, Type::Record(_) | Type::Class(_) | Type::Collection(..)) && !byte_array(ty)
        })
        .collect();
    let array_tag: Vec<&Type> = values
        .iter()
        .filter(|ty| matches!(ty, Type::Array(_) | Type::Tuple(_)) || byte_array(ty))
        .collect();
    let crowded = |members: &[&Type]| {
        members.len() > 1
            && (!shared
                || members
                    .iter()
                    .any(|ty| matches!(ty, Type::Collection(..) | Type::Tuple(_))))
    };
    if crowded(&record_tag)
        || crowded(&array_tag)
        || values
            .iter()
            .filter(|ty| matches!(ty, Type::Dictionary(_)))
            .count()
            > 1
        || values
            .iter()
            .filter(|ty| matches!(ty, Type::Promise(_)))
            .count()
            > 1
        || values
            .iter()
            .filter(|ty| matches!(ty, Type::Function(_)))
            .count()
            > 1
    {
        return Err(unsupported(if types.profile.function_fields() {
            "multiple record shapes, array element types, promise or function types in one union"
        } else {
            "multiple record shapes, array element types or promise types in one union"
        }));
    }
    if shared {
        let kinds: Vec<_> = values
            .iter()
            .filter_map(|ty| match ty {
                Type::Array(id) => Some(
                    types.arrays[*id as usize]
                        .profile_kind(types.profile)
                        .map(|kind| kind as u32),
                ),
                _ => None,
            })
            .collect();
        let mut distinct = kinds.clone();
        distinct.sort();
        distinct.dedup();
        if distinct.len() != kinds.len() || kinds.iter().any(Option::is_none) {
            return Err(unsupported(
                "array members of one union with the same element kind (they are told apart by \
                 element kind, so for example string[] | number[] works but A[] | B[] of two \
                 record types does not)",
            ));
        }
    }
    if values.len() == 1 {
        return Ok(values[0]);
    }
    if let Some(id) = types.unions.iter().position(|u| u == &values) {
        return Ok(if references {
            Type::ReferenceUnion(id as u32)
        } else {
            Type::Union(id as u32)
        });
    }
    if types.unions.len() >= types.profile.limits().union_types {
        return Err(invalid("union type count exceeds limit"));
    }
    let id = types.unions.len() as u32;
    types.unions.push(values);
    literals::note_union(id, types)?;
    Ok(if references {
        Type::ReferenceUnion(id)
    } else {
        Type::Union(id)
    })
}
pub(super) fn widen(value: Expr, expected: Option<Type>, types: &ir::ValueTypes) -> Result<Expr> {
    // `never` (v37) converts to every type: the code is unreachable, so the
    // placeholder of the target type is never observed.
    if value.ty == Type::Never
        && let Some(target) = expected.filter(|target| *target != Type::Never)
    {
        let span = value.span;
        return Ok(Expr {
            span,
            ty: target,
            kind: E::Sequence(
                Box::new(value),
                Box::new(Expr {
                    span,
                    ty: target,
                    kind: E::Zero,
                }),
            ),
        });
    }
    let value = literals::conform(value, expected, types)?;
    // A tuple (v37) is its physical array where that array, or `unknown`, is
    // expected (TypeScript's tuple-to-array assignability).
    let value = match value.ty {
        Type::Tuple(id)
            if expected == Some(types.tuples[id as usize].array)
                || expected == Some(Type::Unknown) =>
        {
            Expr {
                span: value.span,
                ty: types.tuples[id as usize].array,
                kind: E::Reinterpret(Box::new(value)),
            }
        }
        _ => value,
    };
    if expected == Some(Type::Unknown) {
        return unknown::widen(value, types);
    }
    // A non-primitive value as TypeScript's `object` (v38): its cell.
    if expected == Some(Type::Object(false))
        && types.profile.structural_subtyping()
        && value.ty != Type::Object(false)
        && unknown::non_primitive(value.ty, types)
    {
        let cell = unknown::widen(value, types)?;
        return Ok(Expr {
            span: cell.span,
            ty: Type::Object(false),
            kind: E::Reinterpret(Box::new(cell)),
        });
    }
    if let Some(ty @ Type::Class(_)) = expected
        && classes::upcast(types, value.ty, ty)
    {
        return Ok(Expr { ty, ..value });
    }
    // A mixed record and its plain dictionary share one storage (v35).
    if let Some(ty @ Type::Dictionary(_)) = expected
        && dictionaries::converts(ty, value.ty, types)
    {
        return Ok(Expr { ty, ..value });
    }
    // A wider record or class instance viewed as a record shape (v38).
    let value = subtyping::coerce(value, expected, types)?;
    let Some(ty @ (Type::Union(id) | Type::ReferenceUnion(id))) = expected else {
        return Ok(value);
    };
    let target = &types.unions[id as usize];
    let mut value = classes::union_upcast(value, ty, types);
    if !target.contains(&value.ty)
        && let Some(&member) = target
            .iter()
            .find(|member| dictionaries::converts(**member, value.ty, types))
    {
        value.ty = member;
    }
    if members(value.ty, types)
        .iter()
        .any(|m| !literals::member_of(*m, target, types))
    {
        return Err(type_error(format!(
            "value {} is not a member of the expected union",
            value.ty
        )));
    }
    if value.ty.is_union() {
        return Ok(Expr { ty, ..value });
    }
    Ok(Expr {
        span: value.span,
        ty,
        kind: E::BoxUnion(Box::new(value)),
    })
}
impl Lower<'_> {
    pub(super) fn join(&mut self, a: Expr, b: Expr) -> Result<(Type, Expr, Expr)> {
        let ty = intern(vec![a.ty, b.ty], self.types)?;
        Ok((
            ty,
            widen(a, Some(ty), self.types)?,
            widen(b, Some(ty), self.types)?,
        ))
    }
    pub(super) fn truth(&mut self, value: Expr) -> Result<Expr> {
        if self.types.profile.primitive_builtins() {
            if value.ty.result_value().is_some() || value.ty == Type::Void {
                return Err(self.source.locate(type_error("truth conversion requires an ordinary value; opaque result pairs need explicit fields"), value.span));
            }
            return Ok(Expr {
                span: value.span,
                ty: Type::Boolean,
                kind: E::ToBoolean(Box::new(value)),
            });
        }
        if matches!(value.ty, Type::Function(_)) {
            return Ok(Expr {
                span: value.span,
                ty: Type::Boolean,
                kind: E::Sequence(
                    Box::new(value),
                    Box::new(Expr {
                        span: swc_common::DUMMY_SP,
                        ty: Type::Boolean,
                        kind: E::Integer(1),
                    }),
                ),
            });
        }
        if value.ty.is_cell() {
            return Ok(Expr {
                span: value.span,
                ty: Type::Boolean,
                kind: E::Host(
                    self.types.profile.host_operation(Host::UnionTruthy),
                    vec![value],
                ),
            });
        }
        if matches!(value.ty, Type::String | Type::Error | Type::NativeError(_))
            && self.types.profile.unions()
        {
            let ty = intern(vec![value.ty, Type::Undefined], self.types)?;
            return self.truth(widen(value, Some(ty), self.types)?);
        }
        Ok(value)
    }
    pub(super) fn union_binary(
        &mut self,
        op: ast::BinaryOp,
        a: Expr,
        b: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        use ast::BinaryOp::*;
        if matches!(op, LogicalAnd | LogicalOr | NullishCoalescing) {
            let (ty, a, b) = self.join(a, b)?;
            return Ok(Expr {
                span,
                ty,
                kind: E::UnionLogical(op, Box::new(a), Box::new(b)),
            });
        }
        if matches!(op, EqEq | NotEq)
            && (matches!(a.ty, Type::Null | Type::Undefined)
                || matches!(b.ty, Type::Null | Type::Undefined))
        {
            let (_, a, b) = self.join(a, b)?;
            let value = Expr {
                span,
                ty: Type::Boolean,
                kind: E::Host(
                    self.types.profile.host_operation(Host::UnionNullishEqual),
                    vec![a, b],
                ),
            };
            return Ok(if op == NotEq {
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Not(Box::new(value)),
                }
            } else {
                value
            });
        }
        if !matches!(op, EqEqEq | NotEqEq) {
            return Err(unsupported(
                "union operator; narrow the value before arithmetic or coercive comparison",
            ));
        }
        let (_, a, b) = self.join(a, b)?;
        let value = Expr {
            span,
            ty: Type::Boolean,
            kind: E::Host(
                self.types.profile.host_operation(Host::UnionStrictEqual),
                vec![a, b],
            ),
        };
        Ok(if op == NotEqEq {
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Not(Box::new(value)),
            }
        } else {
            value
        })
    }
    /// A guard refines only a direct local. Writes under its refinement are
    /// rejected, so loops cannot reuse stale assumptions after a back edge.
    pub(super) fn nullish_guard(&mut self, test: &ast::Expr) -> Result<()> {
        match test {
            ast::Expr::Paren(p) => self.nullish_guard(&p.expr),
            ast::Expr::Ident(id) => self.refine(id.sym.as_ref(), Guard::Nullish, true),
            _ => Ok(()),
        }
    }
    pub(super) fn guard(&mut self, test: &ast::Expr, yes: bool) -> Result<()> {
        self.flow_guard(test, yes)
    }
    pub(super) fn guard_inner(&mut self, test: &ast::Expr, yes: bool) -> Result<()> {
        if !self.types.profile.unions() {
            return Ok(());
        }
        if let ast::Expr::Paren(p) = test {
            return self.guard(&p.expr, yes);
        }
        if self.types.profile.truth_guards() {
            if let ast::Expr::Ident(id) = test {
                return self.refine(id.sym.as_ref(), Guard::Truth(yes), true);
            }
            if let ast::Expr::Bin(binary) = test
                && ((binary.op == ast::BinaryOp::LogicalAnd && yes)
                    || (binary.op == ast::BinaryOp::LogicalOr && !yes))
            {
                self.guard(&binary.left, yes)?;
                self.guard(&binary.right, yes)?;
                // `x !== null && typeof x === "object"` (v35): the null test
                // also applies to the `object | null` fact proved after it.
                if self.types.profile.unknown_json()
                    && object_narrowing::typeof_object(&binary.right)
                {
                    self.guard(&binary.left, yes)?;
                }
                return Ok(());
            }
            if self.types.profile.literal_types()
                && let ast::Expr::Bin(binary) = test
                && ((binary.op == ast::BinaryOp::LogicalOr && yes)
                    || (binary.op == ast::BinaryOp::LogicalAnd && !yes))
            {
                return self.either_guard(&binary.left, &binary.right, yes);
            }
        }
        if self.types.profile.reference_unions()
            && let ast::Expr::Unary(unary) = test
            && unary.op == ast::UnaryOp::Bang
        {
            return self.guard(&unary.arg, !yes);
        }
        if self.path_guard(test, yes)? {
            return Ok(());
        }
        // A truthy optional chain `x?.p` (v36) proves `x` is not nullish.
        if yes
            && self.types.profile.function_fields()
            && let Some(name) = optional_chains::chained_binding(test)
        {
            return self.refine(&name, Guard::Nullish, false);
        }
        if self.types.profile.reference_unions()
            && let ast::Expr::Call(call) = test
            && let ast::Callee::Expr(callee) = &call.callee
            && let ast::Expr::Member(member) = &**callee
            && matches!(&*member.obj,ast::Expr::Ident(id) if id.sym=="Array")
            && matches!(&member.prop,ast::MemberProp::Ident(id) if id.sym=="isArray")
            && self.binding("Array").is_err()
            && !self.functions.contains_key("Array")
            && call.args.len() == 1
            && let ast::Expr::Ident(id) = &*call.args[0].expr
        {
            return self.refine(id.sym.as_ref(), Guard::Array, yes);
        }
        if let ast::Expr::Bin(binary) = test
            && binary.op == ast::BinaryOp::InstanceOf
            && let Some(class) = self.class_callee(&binary.right)
            && let ast::Expr::Ident(id) = &*binary.left
        {
            return self.refine_class(id.sym.as_ref(), class, yes);
        }
        if let ast::Expr::Bin(binary) = test
            && binary.op == ast::BinaryOp::InstanceOf
            && let Some(class) = self.error_callee(&binary.right)
            && (class == varta_common::value::ErrorClass::Error
                || self.types.profile.error_families())
            && let ast::Expr::Ident(id) = &*binary.left
        {
            if self.types.profile.error_subclasses() {
                return self.refine_error_family(id.sym.as_ref(), class, yes);
            }
            return self.refine(id.sym.as_ref(), Guard::Error(class), yes);
        }
        let ast::Expr::Bin(binary) = test else {
            return Ok(());
        };
        // `#x in value` narrows like `value instanceof C` (v37).
        if binary.op == ast::BinaryOp::In
            && self.types.profile.class_generics()
            && let ast::Expr::PrivateName(name) = &*binary.left
            && let ast::Expr::Ident(id) = &*binary.right
        {
            let class = self.private_owner(name)?;
            return self.refine_class(id.sym.as_ref(), class, yes);
        }
        if binary.op == ast::BinaryOp::In
            && self.property_guard(&binary.left, &binary.right, yes)?
        {
            return Ok(());
        }
        if binary.op == ast::BinaryOp::In && yes && self.types.profile.unknown_json() {
            return self.in_guard(&binary.left, &binary.right);
        }
        let positive = match binary.op {
            ast::BinaryOp::EqEqEq => yes,
            ast::BinaryOp::NotEqEq => !yes,
            _ => return Ok(()),
        };
        if self.types.profile.literal_types()
            && let Some((name, single)) =
                literals::guard_operand(&binary.left, &binary.right, self.types)
                    .or_else(|| literals::guard_operand(&binary.right, &binary.left, self.types))
        {
            return self.refine(&name, Guard::Literal(single), positive);
        }
        let identify = |a: &ast::Expr, b: &ast::Expr| -> Option<(String, Guard)> {
            if let ast::Expr::Unary(u) = a
                && u.op == ast::UnaryOp::TypeOf
                && let ast::Expr::Ident(id) = &*u.arg
                && let ast::Expr::Lit(ast::Lit::Str(text)) = b
            {
                let ty = match text.value.as_str()? {
                    "bigint" if self.types.profile.bigint_values() => Type::BigInt,
                    "number" => Type::Float64,
                    "boolean" => Type::Boolean,
                    "string" => Type::String,
                    "undefined" => Type::Undefined,
                    "object" => return Some((id.sym.to_string(), Guard::Object)),
                    "function" if self.types.profile.function_fields() => {
                        return Some((id.sym.to_string(), Guard::Function));
                    }
                    _ => return None,
                };
                return Some((id.sym.to_string(), Guard::Exact(ty)));
            }
            if let ast::Expr::Ident(id) = a
                && matches!(b, ast::Expr::Lit(ast::Lit::Null(_)))
            {
                return Some((id.sym.to_string(), Guard::Exact(Type::Null)));
            }
            if let ast::Expr::Ident(id) = a
                && let ast::Expr::Ident(undefined) = b
                && undefined.sym == "undefined"
                && self.binding("undefined").is_err()
                && !self.functions.contains_key("undefined")
            {
                return Some((id.sym.to_string(), Guard::Exact(Type::Undefined)));
            }
            None
        };
        let Some((name, matched)) =
            identify(&binary.left, &binary.right).or_else(|| identify(&binary.right, &binary.left))
        else {
            return Ok(());
        };
        self.refine(&name, matched, positive)
    }
}
