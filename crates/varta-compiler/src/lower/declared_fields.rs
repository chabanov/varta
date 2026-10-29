//! Checked declared fields (v36 `checked_declared_fields`): converting an
//! `unknown` cell to any declared type that `JSON.parse(text) as T` could
//! describe, for the declared fields of mixed records indexed by `unknown`,
//! their checked parsing and checked assertions from `unknown`.
//!
//! A conversion first tests whether the cell provably holds a value whose
//! static type is the target (*exact*); then the stored value itself is the
//! result, so identity and mutation behave as in JavaScript. Records,
//! dictionaries, primitives and class instances are proven by their tag and
//! descriptor. An array is proven only when its element kind matches and,
//! for record, dictionary or array elements, it is non-empty, has no holes and
//! every element is exact: an empty array or an array of union cells could be
//! another alias's `Other[]` or `unknown[]`, and returning it as `T[]` would
//! let writes through one alias break the other's element type.
//!
//! Otherwise the conversion *builds* a fresh value of the target from the
//! JSON-graph form of the cell: a `Record<string, unknown>` dictionary becomes
//! a structural record (exactly its declared keys, in the dictionary's order)
//! or a dictionary of another value type, and an array of cells (or of the
//! target's element kinds) becomes a fresh dense `T[]` whose elements are
//! converted in turn (shared when exact). Holes read as `undefined`. Any other
//! value throws `TypeError(message)`. Each array, record and dictionary build
//! and each array exactness test is one synthesized function per target type,
//! so recursive types convert through recursive calls. No value tag, layout,
//! host import or budget is added; the work is ordinary guest code bounded by
//! fuel, memory and the JSON depth/entry limits of the converted values.
use super::*;
use ir::Converter;
use std::collections::BTreeSet;
use swc_common::Span;
use varta_common::value::{ErrorClass, ValueKind};

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
fn number(span: Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn test(span: Span, kind: E) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind,
    }
}
fn binary(op: ast::BinaryOp, left: Expr, right: Expr) -> Expr {
    let span = left.span;
    let ty = if matches!(op, ast::BinaryOp::Add) {
        Type::Float64
    } else {
        Type::Boolean
    };
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
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
fn not(span: Span, value: Expr) -> Expr {
    test(span, E::Not(Box::new(value)))
}
fn length(array: Expr) -> Expr {
    Expr {
        span: array.span,
        ty: Type::Float64,
        kind: E::Length(Box::new(array)),
    }
}
fn increment(span: Span, slot: u32) -> S {
    S::Set(
        slot,
        binary(
            ast::BinaryOp::Add,
            local(span, slot, Type::Float64),
            number(span, 1.0),
        ),
    )
}
fn push(span: Span, array: Expr, item: Expr) -> S {
    let ty = array.ty;
    S::Expr(Expr {
        span,
        ty: Type::Float64,
        kind: E::ArrayPush(
            Box::new(array),
            Box::new(Expr {
                span,
                ty,
                kind: E::Array(vec![Some(item)]),
            }),
        ),
    })
}
/// The cell in `slot` viewed as `target`: retagged when `target` is a cell,
/// unboxed otherwise. Valid only after a test proved the member.
fn view(span: Span, slot: u32, target: Type) -> Expr {
    if target.is_cell() {
        local(span, slot, target)
    } else {
        Expr {
            span,
            ty: target,
            kind: E::UnboxUnion(Box::new(local(span, slot, Type::Unknown))),
        }
    }
}

/// Primitive element types whose array element kind alone proves them.
fn primitive_element(ty: Type) -> Option<ValueKind> {
    match ty {
        Type::Float64
        | Type::Boolean
        | Type::String
        | Type::Null
        | Type::Undefined
        | Type::BigInt => ty.value_kind(),
        _ => None,
    }
}

/// Element types that an exact array proves element by element: each element's
/// descriptor (structural records, dictionaries) or its own exactness (arrays)
/// determines the element type, so a non-empty array of them has one static type.
fn proven_elements(ty: Type, types: &ir::ValueTypes) -> bool {
    match ty {
        Type::Record(shape) => types.records[shape as usize].class.is_none(),
        Type::Dictionary(_) | Type::Array(_) => true,
        _ => false,
    }
}

/// Whether a member of a target is built (not only proven) from the JSON
/// graph: structural records, arrays and dictionaries of a value type other
/// than `unknown` (a `Record<string, unknown>` is always exact for one).
fn built(ty: Type, types: &ir::ValueTypes) -> bool {
    match ty {
        Type::Record(shape) => types.records[shape as usize].class.is_none(),
        Type::Array(_) => true,
        Type::Dictionary(id) => types.dictionaries[id as usize].value != Type::Unknown,
        _ => false,
    }
}

/// Whether some member of `ty` is built from the JSON graph, so a checked read
/// or assertion at `ty` converts instead of only testing the cell.
pub(super) fn buildable(ty: Type, types: &ir::ValueTypes) -> bool {
    unions::members(ty, types)
        .into_iter()
        .any(|member| built(member, types))
}

/// Whether every value of `ty` can be proven or built from an `unknown` cell:
/// primitives, literals, `null`, `undefined`, BigInt, errors, class
/// instances, `unknown`, and structural records, arrays, dictionaries, mixed
/// records and closed unions of these. Functions, generators, promises and
/// `object` views are not.
pub(super) fn convertible(ty: Type, types: &ir::ValueTypes) -> bool {
    convertible_visit(ty, types, &mut BTreeSet::new())
}
fn convertible_visit(ty: Type, types: &ir::ValueTypes, seen: &mut BTreeSet<Type>) -> bool {
    if !seen.insert(ty) {
        return true;
    }
    match ty {
        Type::Unknown
        | Type::Float64
        | Type::Boolean
        | Type::String
        | Type::Null
        | Type::Undefined
        | Type::BigInt
        | Type::Error
        | Type::NativeError(_)
        | Type::Literal(..)
        | Type::Class(_) => true,
        Type::Union(id) | Type::ReferenceUnion(id) => types.unions[id as usize]
            .iter()
            .all(|member| convertible_visit(*member, types, seen)),
        Type::Record(shape) => {
            let shape = &types.records[shape as usize];
            shape.class.is_none()
                && shape
                    .fields
                    .iter()
                    .all(|field| convertible_visit(field.ty, types, seen))
        }
        Type::Array(id) => convertible_visit(types.arrays[id as usize], types, seen),
        Type::Dictionary(id) => {
            let value = types.dictionaries[id as usize].value;
            types.declared(ty).is_none_or(|members| {
                !members.object
                    && members
                        .fields
                        .iter()
                        .all(|field| convertible_visit(field.ty, types, seen))
            }) && convertible_visit(value, types, seen)
        }
        _ => false,
    }
}

/// Whether a checked read or assertion from `source` to `target` converts
/// (v36): the source is an `unknown` cell and the target has a built member.
pub(super) fn applies(source: Type, target: Type, types: &ir::ValueTypes) -> bool {
    types.profile.checked_declared_fields()
        && source == Type::Unknown
        && buildable(target, types)
        && convertible(target, types)
}

/// Whether two expressions name the same property path: an identifier or
/// `this`, then static member keys (`a.b`, `a["b"]`).
fn same_path(a: &ast::Expr, b: &ast::Expr) -> bool {
    match (stmt::unparen(a), stmt::unparen(b)) {
        (ast::Expr::Ident(a), ast::Expr::Ident(b)) => a.sym == b.sym,
        (ast::Expr::This(_), ast::Expr::This(_)) => true,
        (ast::Expr::Member(a), ast::Expr::Member(b)) => {
            mixed_records::static_key(a)
                .is_some_and(|key| mixed_records::static_key(b).as_deref() == Some(key.as_str()))
                && same_path(&a.obj, &b.obj)
        }
        _ => false,
    }
}
/// The member type a `typeof x === "<name>"` test proves.
fn typeof_member(name: &str) -> Option<Type> {
    Some(match name {
        "string" => Type::String,
        "number" => Type::Float64,
        "boolean" => Type::Boolean,
        "bigint" => Type::BigInt,
        "undefined" => Type::Undefined,
        _ => return None,
    })
}

/// The locals and statements of one build function body: the source cell,
/// the message and the output, every rooted local and the statements so far.
struct BuildState {
    span: Span,
    target: Type,
    saved: u32,
    message: u32,
    output: u32,
    roots: Vec<u32>,
    body: Vec<S>,
}

/// The rooted key and value vectors of a `Record<string, unknown>` source.
struct GraphObject {
    keys: u32,
    keys_ty: Type,
    values: u32,
    values_ty: Type,
}

impl Lower<'_> {
    /// The context of an array or object literal fallback in `a ?? []` or
    /// `a || {}` (v36): the one aggregate member of `a`'s type, as TypeScript
    /// infers `Item[] | never[]`, so an empty literal gets an element type.
    pub(super) fn fallback_context(&self, binary: &ast::BinExpr, left: Type) -> Option<Type> {
        if !self.types.profile.checked_declared_fields()
            || !matches!(
                binary.op,
                ast::BinaryOp::NullishCoalescing | ast::BinaryOp::LogicalOr
            )
        {
            return None;
        }
        let array = match stmt::unparen(&binary.right) {
            ast::Expr::Array(_) => true,
            ast::Expr::Object(_) => false,
            _ => return None,
        };
        let mut members = unions::members(left, self.types).into_iter().filter(|ty| {
            if array {
                matches!(ty, Type::Array(_))
            } else {
                matches!(ty, Type::Record(_) | Type::Dictionary(_))
            }
        });
        let member = members.next()?;
        members.next().is_none().then_some(member)
    }
    /// `typeof a.b === "string" ? a.b : other` (v36), and `!==` with the
    /// path in the alternate: TypeScript narrows the property path, which
    /// Varta's flow facts (direct locals only) cannot. The property of a
    /// record or dictionary is read once into a hidden local, tested by its
    /// tag and read narrowed in the selected branch. Such reads have no
    /// side effects (records and dictionaries have no accessors), so reading
    /// once is indistinguishable from JavaScript's two reads. `None` when the
    /// conditional is not this idiom.
    pub(super) fn narrowed_path_conditional(
        &mut self,
        branch: &ast::CondExpr,
        expected: Option<Type>,
    ) -> Result<Option<Expr>> {
        if !self.types.profile.checked_declared_fields() {
            return Ok(None);
        }
        let ast::Expr::Bin(test) = stmt::unparen(&branch.test) else {
            return Ok(None);
        };
        let positive = match test.op {
            ast::BinaryOp::EqEqEq => true,
            ast::BinaryOp::NotEqEq => false,
            _ => return Ok(None),
        };
        fn typed(e: &ast::Expr) -> Option<&ast::Expr> {
            match stmt::unparen(e) {
                ast::Expr::Unary(unary) if unary.op == ast::UnaryOp::TypeOf => Some(&*unary.arg),
                _ => None,
            }
        }
        let name = |e: &ast::Expr| match stmt::unparen(e) {
            ast::Expr::Lit(ast::Lit::Str(text)) => text.value.as_str().map(str::to_owned),
            _ => None,
        };
        let (Some(path), Some(name)) = (
            typed(&test.left).or_else(|| typed(&test.right)),
            name(&test.right).or_else(|| name(&test.left)),
        ) else {
            return Ok(None);
        };
        let (narrowed, other) = if positive {
            (&branch.cons, &branch.alt)
        } else {
            (&branch.alt, &branch.cons)
        };
        let (ast::Expr::Member(member), Some(member_ty)) =
            (stmt::unparen(path), typeof_member(&name))
        else {
            return Ok(None);
        };
        if !same_path(path, narrowed) || mixed_records::static_key(member).is_none() {
            return Ok(None);
        }
        // Only record and dictionary properties: class members may be accessors.
        // The path is identifiers, `this` and static keys, so lowering its
        // object twice (here and on the fallback) has no effect.
        let base = self.expr(&member.obj)?;
        let value = match base.ty {
            Type::Record(_) => self.record_field(member, base, true)?,
            Type::Dictionary(_) => self.dictionary_read(member, base)?,
            _ => return Ok(None),
        };
        let span = branch.span;
        let source = value.ty;
        let members: Vec<Type> = unions::members(source, self.types)
            .into_iter()
            .filter(|ty| ty.erased() == member_ty)
            .collect();
        let target = if source == Type::Unknown {
            member_ty
        } else if members.is_empty() {
            return Ok(None);
        } else {
            unions::intern(members, self.types)?
        };
        let saved = self.hidden(source);
        let read = local(span, saved, source);
        let check = if source.is_cell() {
            self.cell_test(span, saved, source, target, &name)?
        } else {
            boolean(span, true)
        };
        let proven = if source.is_cell() && !target.is_cell() {
            Expr {
                span,
                ty: target,
                kind: E::UnboxUnion(Box::new(read)),
            }
        } else if source.is_cell() {
            local(span, saved, target)
        } else {
            read
        };
        // The other branch may not run: join its flow facts with the state
        // of the proven branch, as an ordinary conditional does.
        let entry = self.flow_snapshot();
        let other = self.expr_contextual(other, expected, None)?;
        let after = self.flow_snapshot();
        self.flow_join(&entry, &[entry.clone(), after])?;
        let (ty, proven, other) = if proven.ty == other.ty {
            (proven.ty, proven, other)
        } else {
            self.join(proven, other)?
        };
        // The check tests membership, so the proven read is its true branch
        // for both `===` (consequent) and `!==` (alternate).
        let (yes, no) = (proven, other);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Some(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, value),
                    S::Init(
                        output,
                        Expr {
                            span,
                            ty,
                            kind: E::Conditional(Box::new(check), Box::new(yes), Box::new(no)),
                        },
                    ),
                ],
            },
        }))
    }
    /// The declared read `receiver[key]` of a mixed record of `unknown` whose
    /// field has type `target` (v36): a call of the shared read function with
    /// the `TypeError` messages for a wrong and a missing entry.
    pub(super) fn declared_call(
        &mut self,
        span: Span,
        mut receiver: Expr,
        key: Expr,
        target: Type,
        wrong: &str,
        missing: &str,
    ) -> Result<Expr> {
        let key = self.dictionary_key_value(key)?;
        // Declared members are a static view of the plain dictionary's storage.
        receiver.ty = dictionaries::intern(Type::Unknown, self.types)?;
        let index = self.converter(Converter::Read, target, span)?;
        let text = |this: &Self, message: &str| Expr {
            span,
            ty: Type::String,
            kind: E::String(this.types.profile.string_bytes(message)),
        };
        let (wrong, missing) = (text(self, wrong), text(self, missing));
        Ok(Expr {
            span,
            ty: target,
            kind: E::Call(index, vec![receiver, key, wrong, missing]),
        })
    }
    /// The body of [`Converter::Read`]: look the key up; a present entry is
    /// converted (and a built value written back, keeping the entry's
    /// position); an absent one reads `undefined` when `target` admits it and
    /// throws the missing message otherwise.
    fn read_body(&mut self, span: Span, target: Type, args: Vec<Expr>) -> Result<Vec<S>> {
        let [receiver, key, wrong, missing] =
            <[Expr; 4]>::try_from(args).map_err(|_| invalid("declared read arity"))?;
        let (found, index, mut block) = self.dictionary_lookup(receiver, key)?;
        let element = found.ty;
        let output = self.hidden(target);
        let saved = self.hidden(Type::Unknown);
        let wrong_text = self.hidden(Type::String);
        let missing_text = self.hidden(Type::String);
        for slot in [saved, wrong_text, missing_text] {
            block.root(slot);
        }
        block.body.push(S::Init(wrong_text, wrong));
        block.body.push(S::Init(missing_text, missing));
        let stored = unions::widen(local(span, output, target), Some(element), self.types)?;
        let write = self.dictionary_set(block.receiver(), block.key_value(), stored, element)?;
        let convert = self.convert_into(
            span,
            saved,
            target,
            output,
            wrong_text,
            vec![S::Expr(write)],
        )?;
        let absent = if mixed_records::admits_undefined(target, self.types) {
            let undefined = Expr {
                span,
                ty: Type::Undefined,
                kind: E::Integer(0),
            };
            S::Init(output, unions::widen(undefined, Some(target), self.types)?)
        } else {
            self.conversion_error(span, missing_text)?
        };
        block.body.push(S::If(
            index,
            vec![S::Init(saved, found), convert],
            vec![absent],
        ));
        Ok(vec![S::Return(Some(
            self.finish_dictionary(block, output, target),
        ))])
    }
    /// `value` (an `unknown` cell) converted to `target`; a failure throws
    /// `TypeError(message)`. Used by v36 checked assertions.
    pub(super) fn converted_value(
        &mut self,
        value: Expr,
        target: Type,
        message: &str,
        span: Span,
    ) -> Result<Expr> {
        let saved = self.hidden(Type::Unknown);
        let text = self.hidden(Type::String);
        let output = self.hidden(target);
        let marker = self.hidden(Type::Number);
        let convert = self.convert_into(span, saved, target, output, text, Vec::new())?;
        Ok(Expr {
            span,
            ty: target,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved, text],
                body: vec![
                    S::Init(saved, value),
                    S::Init(
                        text,
                        Expr {
                            span,
                            ty: Type::String,
                            kind: E::String(self.types.profile.string_bytes(message)),
                        },
                    ),
                    convert,
                ],
            },
        })
    }
    /// Convert the `unknown` cell in the rooted local `saved` into `output`:
    /// the cell itself when it is exact for `target`, otherwise the built
    /// value followed by `after_build` (a declared read's write-back). The
    /// `TypeError` message is the string in the rooted local `message`.
    pub(super) fn convert_into(
        &mut self,
        span: Span,
        saved: u32,
        target: Type,
        output: u32,
        message: u32,
        after_build: Vec<S>,
    ) -> Result<S> {
        let exact = self.exact_test(span, saved, target)?;
        let mut build = self.build_statements(span, saved, target, output, message)?;
        build.extend(after_build);
        Ok(S::If(
            exact,
            vec![S::Init(output, view(span, saved, target))],
            build,
        ))
    }
    /// `TypeError(message)` thrown with the string in local `message`.
    fn conversion_error(&mut self, span: Span, message: u32) -> Result<S> {
        let text = unions::intern(vec![Type::String, Type::Undefined], self.types)?;
        let text = unions::widen(local(span, message, Type::String), Some(text), self.types)?;
        let error = Expr {
            span,
            ty: Type::from_error_class(ErrorClass::TypeError),
            kind: E::NewError(ErrorClass::TypeError, Box::new(text)),
        };
        let thrown = self.exception_type()?;
        Ok(S::Throw(unions::widen(error, Some(thrown), self.types)?))
    }
    /// Whether the cell in `slot` provably holds a value of static type
    /// `target`, so that value may be returned as it is.
    fn exact_test(&mut self, span: Span, slot: u32, target: Type) -> Result<Expr> {
        match target {
            Type::Unknown => Ok(boolean(span, true)),
            Type::Union(_) | Type::ReferenceUnion(_) => {
                let mut check = boolean(span, false);
                for member in unions::members(target, self.types).into_iter().rev() {
                    let next = self.exact_test(span, slot, member)?;
                    check = or(span, next, check);
                }
                Ok(check)
            }
            Type::Array(_) => {
                let member = test(
                    span,
                    E::IsUnionMember(Box::new(local(span, slot, Type::Unknown)), target),
                );
                let exact = self.exact_array(span, view(span, slot, target), target)?;
                Ok(and(span, member, exact))
            }
            _ => self.cell_test(span, slot, Type::Unknown, target, "a declared type"),
        }
    }
    /// Whether the array `value` (typed `target`; its array tag is already
    /// proven) has static type `target`.
    fn exact_array(&mut self, span: Span, value: Expr, target: Type) -> Result<Expr> {
        let Type::Array(id) = target else {
            return Err(invalid("missing exact array type"));
        };
        let element = self.types.arrays[id as usize];
        if let Some(kind) = primitive_element(element) {
            return Ok(test(span, E::ArrayKind(Box::new(value), kind)));
        }
        if !proven_elements(element, self.types) {
            return Ok(boolean(span, false));
        }
        let index = self.converter(Converter::Exact, target, span)?;
        Ok(test(span, E::Call(index, vec![value])))
    }
    /// Whether one element `value` of an exact array candidate is exact.
    fn exact_element(&mut self, span: Span, value: Expr, element: Type) -> Result<Expr> {
        let shape = match element {
            Type::Record(shape) => shape,
            Type::Dictionary(id) => self.types.dictionaries[id as usize].storage,
            Type::Array(_) => return self.exact_array(span, value, element),
            _ => return Ok(boolean(span, false)),
        };
        Ok(test(
            span,
            E::IsClass {
                value: Box::new(value),
                shapes: vec![shape],
                slot: self.hidden(Type::Number),
            },
        ))
    }
    /// The shared function of `kind` for `target`, synthesized on first use.
    /// The index is registered before the body is lowered, so recursive types
    /// call themselves.
    fn converter(&mut self, kind: Converter, target: Type, span: Span) -> Result<usize> {
        if let Some(index) = self.types.converters.get(&(kind, target)) {
            return Ok(*index);
        }
        let index =
            self.reserve_function("too many functions including declared-field conversions")?;
        self.types.converters.insert((kind, target), index);
        match kind {
            Converter::Exact => {
                self.define_function(index, span, vec![target], Type::Boolean, |child, args| {
                    child.exact_array_body(span, target, args)
                })?
            }
            Converter::Build => self.define_function(
                index,
                span,
                vec![Type::Unknown, Type::String],
                target,
                |child, args| child.build_body(span, target, args),
            )?,
            Converter::Read => {
                let graph = dictionaries::intern(Type::Unknown, self.types)?;
                self.define_function(
                    index,
                    span,
                    vec![graph, Type::String, Type::String, Type::String],
                    target,
                    |child, args| child.read_body(span, target, args),
                )?
            }
        }
        Ok(index)
    }
    /// `(array) -> boolean` of [`Converter::Exact`]: the element kind matches,
    /// the array is non-empty and dense, and every element is exact.
    fn exact_array_body(&mut self, span: Span, target: Type, args: Vec<Expr>) -> Result<Vec<S>> {
        let [array] = <[Expr; 1]>::try_from(args).map_err(|_| invalid("exact array arity"))?;
        let Type::Array(id) = target else {
            return Err(invalid("missing exact array type"));
        };
        let element = self.types.arrays[id as usize];
        let kind = element
            .value_kind()
            .ok_or_else(|| invalid("missing exact array element kind"))?;
        let saved = self.hidden(target);
        let output = self.hidden(Type::Boolean);
        let index = self.hidden(Type::Float64);
        let count = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        let at = || local(span, index, Type::Float64);
        let item = Expr {
            span,
            ty: element,
            kind: E::Index(Box::new(local(span, saved, target)), Box::new(at())),
        };
        let exact = self.exact_element(span, item, element)?;
        let present = test(
            span,
            E::ArrayHasOwn(Box::new(local(span, saved, target)), Box::new(at())),
        );
        let scan = vec![
            S::Set(output, boolean(span, true)),
            S::Init(index, number(span, 0.0)),
            S::While(
                and(
                    span,
                    local(span, output, Type::Boolean),
                    binary(ast::BinaryOp::Lt, at(), local(span, count, Type::Float64)),
                ),
                vec![
                    S::If(
                        present,
                        vec![S::Set(output, exact)],
                        vec![S::Set(output, boolean(span, false))],
                    ),
                    increment(span, index),
                ],
            ),
        ];
        let body = vec![
            S::Init(saved, array),
            S::Init(output, boolean(span, false)),
            S::If(
                test(
                    span,
                    E::ArrayKind(Box::new(local(span, saved, target)), kind),
                ),
                vec![
                    S::Init(count, length(local(span, saved, target))),
                    S::If(
                        binary(
                            ast::BinaryOp::Gt,
                            local(span, count, Type::Float64),
                            number(span, 0.0),
                        ),
                        scan,
                        vec![],
                    ),
                ],
                vec![],
            ),
        ];
        Ok(vec![S::Return(Some(Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body,
            },
        }))])
    }
    /// Statements that store the value built from the (non-exact) cell in
    /// `saved` into `output`, or throw. Unions dispatch on the cell's tag to
    /// their array member or, for a dictionary, to their dictionary member
    /// and else their record member.
    fn build_statements(
        &mut self,
        span: Span,
        saved: u32,
        target: Type,
        output: u32,
        message: u32,
    ) -> Result<Vec<S>> {
        if built(target, self.types) {
            let index = self.converter(Converter::Build, target, span)?;
            return Ok(vec![S::Init(
                output,
                Expr {
                    span,
                    ty: target,
                    kind: E::Call(
                        index,
                        vec![
                            local(span, saved, Type::Unknown),
                            local(span, message, Type::String),
                        ],
                    ),
                },
            )]);
        }
        let members = unions::members(target, self.types);
        if !target.is_union() || !buildable(target, self.types) {
            return Ok(vec![self.conversion_error(span, message)?]);
        }
        let find = |pick: fn(&Type) -> bool| members.iter().copied().find(pick);
        let array = find(|ty| matches!(ty, Type::Array(_)));
        let dictionary =
            find(|ty| matches!(ty, Type::Dictionary(_))).filter(|ty| built(*ty, self.types));
        let record = find(|ty| matches!(ty, Type::Record(_))).filter(|ty| built(*ty, self.types));
        let member = |this: &mut Self, ty: Option<Type>| -> Result<Vec<S>> {
            let Some(ty) = ty else {
                return Ok(vec![this.conversion_error(span, message)?]);
            };
            let value = this.hidden(ty);
            let mut statements = this.build_statements(span, saved, ty, value, message)?;
            // Widening boxes the built member; boxing roots it first.
            statements.push(S::Init(
                output,
                unions::widen(local(span, value, ty), Some(target), this.types)?,
            ));
            Ok(statements)
        };
        let arrays = member(self, array)?;
        let objects = member(self, dictionary.or(record))?;
        let otherwise = vec![self.conversion_error(span, message)?];
        let any_dictionary = dictionaries::intern(Type::Unknown, self.types)?;
        let any_array = arrays::intern(Type::Unknown, self.types)?;
        let tag = |ty: Type| {
            test(
                span,
                E::IsUnionMember(Box::new(local(span, saved, Type::Unknown)), ty),
            )
        };
        Ok(vec![S::If(
            tag(any_array),
            arrays,
            vec![S::If(tag(any_dictionary), objects, otherwise)],
        )])
    }
    /// `(cell, message) -> T` of [`Converter::Build`].
    fn build_body(&mut self, span: Span, target: Type, args: Vec<Expr>) -> Result<Vec<S>> {
        let [cell, message] =
            <[Expr; 2]>::try_from(args).map_err(|_| invalid("build conversion arity"))?;
        let saved = self.hidden(Type::Unknown);
        let text = self.hidden(Type::String);
        let output = self.hidden(target);
        let marker = self.hidden(Type::Number);
        let mut state = BuildState {
            span,
            target,
            saved,
            message: text,
            output,
            roots: vec![saved, text, output],
            body: vec![S::Init(saved, cell), S::Init(text, message)],
        };
        match target {
            Type::Array(_) => self.build_array(&mut state)?,
            Type::Record(_) => self.build_record(&mut state)?,
            Type::Dictionary(_) => self.build_dictionary(&mut state)?,
            _ => return Err(invalid("unsupported declared-field build target")),
        }
        let BuildState { roots, body, .. } = state;
        Ok(vec![S::Return(Some(Expr {
            span,
            ty: target,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        }))])
    }
    /// `value` (an `unknown` cell) converted to `target` inside a build
    /// function whose message is in local `message`.
    fn convert_part(
        &mut self,
        span: Span,
        value: Expr,
        target: Type,
        message: u32,
    ) -> Result<Expr> {
        let saved = self.hidden(Type::Unknown);
        let output = self.hidden(target);
        let marker = self.hidden(Type::Number);
        let convert = self.convert_into(span, saved, target, output, message, Vec::new())?;
        Ok(Expr {
            span,
            ty: target,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![S::Init(saved, value), convert],
            },
        })
    }
    /// The physical element types a source array may be read at to build an
    /// array of `element`: union cells as `unknown`, and each kind of a
    /// non-cell member of `element` (all primitives for `unknown`).
    fn readers(&self, element: Type) -> Vec<(ValueKind, Type)> {
        let mut readers = vec![
            (ValueKind::ReferenceUnion, Type::Unknown),
            (ValueKind::PrimitiveUnion, Type::Unknown),
        ];
        let members = if element == Type::Unknown {
            vec![
                Type::Float64,
                Type::Boolean,
                Type::String,
                Type::Null,
                Type::Undefined,
                Type::BigInt,
            ]
        } else {
            unions::members(element, self.types)
        };
        for member in members {
            let physical = member.erased();
            if physical.is_cell() {
                continue;
            }
            if let Some(kind) = physical.value_kind()
                && !readers.iter().any(|(known, _)| *known == kind)
            {
                readers.push((kind, physical));
            }
        }
        readers
    }
    /// A fresh dense `T[]` from any array whose element kind is a reader of
    /// `T`, each element converted (holes read as `undefined`).
    fn build_array(&mut self, state: &mut BuildState) -> Result<()> {
        let (span, target, saved, message, output) = (
            state.span,
            state.target,
            state.saved,
            state.message,
            state.output,
        );
        let Type::Array(id) = target else {
            return Err(invalid("missing array build type"));
        };
        let element = self.types.arrays[id as usize];
        let index = self.hidden(Type::Float64);
        let count = self.hidden(Type::Float64);
        let item = self.hidden(element);
        state.roots.push(item);
        let wrong = self.conversion_error(span, message)?;
        state.body.push(S::If(
            not(
                span,
                test(
                    span,
                    E::IsUnionMember(Box::new(local(span, saved, Type::Unknown)), target),
                ),
            ),
            vec![wrong],
            vec![],
        ));
        state.body.push(S::Init(
            output,
            Expr {
                span,
                ty: target,
                kind: E::Array(Vec::new()),
            },
        ));
        state.body.push(S::Init(index, number(span, 0.0)));
        let mut chain = vec![self.conversion_error(span, message)?];
        for (kind, physical) in self.readers(element).into_iter().rev() {
            let array = arrays::intern(physical, self.types)?;
            let source = self.hidden(array);
            state.roots.push(source);
            let maybe = unions::intern(vec![physical, Type::Undefined], self.types)?;
            let read = Expr {
                span,
                ty: maybe,
                kind: E::MaybeIndex(
                    Box::new(local(span, source, array)),
                    Box::new(local(span, index, Type::Float64)),
                ),
            };
            let cell = unions::widen(read, Some(Type::Unknown), self.types)?;
            let converted = self.convert_part(span, cell, element, message)?;
            let step = vec![
                S::Init(item, converted),
                push(
                    span,
                    local(span, output, target),
                    local(span, item, element),
                ),
                increment(span, index),
            ];
            let branch = vec![
                S::Init(source, view(span, saved, array)),
                S::Init(count, length(local(span, source, array))),
                S::While(
                    binary(
                        ast::BinaryOp::Lt,
                        local(span, index, Type::Float64),
                        local(span, count, Type::Float64),
                    ),
                    step,
                ),
            ];
            chain = vec![S::If(
                test(span, E::ArrayKind(Box::new(view(span, saved, array)), kind)),
                branch,
                chain,
            )];
        }
        state.body.extend(chain);
        Ok(())
    }
    /// The `Record<string, unknown>` dictionary held by the cell in `saved`,
    /// or a `TypeError`: its key and value vectors in fresh rooted locals.
    fn graph_object(&mut self, state: &mut BuildState) -> Result<GraphObject> {
        let (span, saved, message) = (state.span, state.saved, state.message);
        let graph = dictionaries::intern(Type::Unknown, self.types)?;
        let Type::Dictionary(id) = graph else {
            return Err(invalid("missing dictionary type"));
        };
        let dictionary = self.types.dictionaries[id as usize];
        let check = self.cell_test(span, saved, Type::Unknown, graph, "a JSON object")?;
        let wrong = self.conversion_error(span, message)?;
        let source = self.hidden(graph);
        let keys = self.hidden(dictionary.keys);
        let values = self.hidden(dictionary.values);
        state.roots.extend([source, keys, values]);
        let field = |slot: u32, ty: Type| Expr {
            span,
            ty,
            kind: E::Field(Box::new(local(span, source, graph)), slot),
        };
        state.body.extend([
            S::If(not(span, check), vec![wrong], vec![]),
            S::Init(source, view(span, saved, graph)),
            S::Init(keys, field(dictionaries::KEYS, dictionary.keys)),
            S::Init(values, field(dictionaries::VALUES, dictionary.values)),
        ]);
        Ok(GraphObject {
            keys,
            keys_ty: dictionary.keys,
            values,
            values_ty: dictionary.values,
        })
    }
    /// A fresh structural record from a `Record<string, unknown>`: every key
    /// must be a declared field (as in checked parsing), each value converted
    /// to its field type and stored in the dictionary's key order; a missing
    /// required field throws.
    fn build_record(&mut self, state: &mut BuildState) -> Result<()> {
        let (span, target, message, output) =
            (state.span, state.target, state.message, state.output);
        let Type::Record(shape) = target else {
            return Err(invalid("missing record build type"));
        };
        let fields = self.types.records[shape as usize].fields.clone();
        let GraphObject {
            keys,
            keys_ty,
            values,
            values_ty,
        } = self.graph_object(state)?;
        let mut initial = Vec::new();
        for (slot, field) in fields.iter().enumerate() {
            let value = if field.optional {
                let undefined = Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                };
                unions::widen(undefined, Some(field.ty), self.types)?
            } else {
                Expr {
                    span,
                    ty: field.ty,
                    kind: E::Zero,
                }
            };
            initial.push((slot as u32, value));
        }
        state.body.push(S::Init(
            output,
            Expr {
                span,
                ty: target,
                kind: E::OptionalRecord(initial, 0),
            },
        ));
        let index = self.hidden(Type::Float64);
        let count = self.hidden(Type::Float64);
        let key = self.hidden(Type::String);
        state.roots.push(key);
        let seen: Vec<Option<u32>> = fields
            .iter()
            .map(|field| (!field.optional).then(|| self.hidden(Type::Boolean)))
            .collect();
        for flag in seen.iter().flatten() {
            state.body.push(S::Init(*flag, boolean(span, false)));
        }
        let at = || local(span, index, Type::Float64);
        let mut chain = vec![self.conversion_error(span, message)?];
        for (slot, field) in fields.iter().enumerate().rev() {
            let entry = Expr {
                span,
                ty: Type::Unknown,
                kind: E::Index(Box::new(local(span, values, values_ty)), Box::new(at())),
            };
            let converted = self.convert_part(span, entry, field.ty, message)?;
            let mut store = vec![S::SetField(
                local(span, output, target),
                slot as u32,
                converted,
            )];
            if let Some(flag) = seen[slot] {
                store.push(S::Set(flag, boolean(span, true)));
            }
            let name = Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(&field.name)),
            };
            chain = vec![S::If(
                binary(ast::BinaryOp::EqEqEq, local(span, key, Type::String), name),
                store,
                chain,
            )];
        }
        let mut step = vec![S::Init(
            key,
            Expr {
                span,
                ty: Type::String,
                kind: E::Index(Box::new(local(span, keys, keys_ty)), Box::new(at())),
            },
        )];
        step.extend(chain);
        step.push(increment(span, index));
        state.body.extend([
            S::Init(index, number(span, 0.0)),
            S::Init(count, length(local(span, keys, keys_ty))),
            S::While(
                binary(ast::BinaryOp::Lt, at(), local(span, count, Type::Float64)),
                step,
            ),
        ]);
        for flag in seen.into_iter().flatten() {
            let missing = self.conversion_error(span, message)?;
            state.body.push(S::If(
                not(span, local(span, flag, Type::Boolean)),
                vec![missing],
                vec![],
            ));
        }
        Ok(())
    }
    /// A fresh dictionary (or mixed record) of value type `V` from a
    /// `Record<string, unknown>`: the keys in order, each value converted.
    /// Declared fields of a mixed target stay a checked view.
    fn build_dictionary(&mut self, state: &mut BuildState) -> Result<()> {
        let (span, target, message, output) =
            (state.span, state.target, state.message, state.output);
        let Type::Dictionary(id) = target else {
            return Err(invalid("missing dictionary build type"));
        };
        let dictionary = self.types.dictionaries[id as usize];
        let GraphObject {
            keys,
            keys_ty,
            values,
            values_ty,
        } = self.graph_object(state)?;
        let new_keys = self.hidden(dictionary.keys);
        let new_values = self.hidden(dictionary.values);
        let item = self.hidden(dictionary.value);
        let index = self.hidden(Type::Float64);
        let count = self.hidden(Type::Float64);
        state.roots.extend([new_keys, new_values, item]);
        let at = || local(span, index, Type::Float64);
        let entry = Expr {
            span,
            ty: Type::Unknown,
            kind: E::Index(Box::new(local(span, values, values_ty)), Box::new(at())),
        };
        let converted = self.convert_part(span, entry, dictionary.value, message)?;
        let key = Expr {
            span,
            ty: Type::String,
            kind: E::Index(Box::new(local(span, keys, keys_ty)), Box::new(at())),
        };
        let empty = |ty: Type| Expr {
            span,
            ty,
            kind: E::Array(Vec::new()),
        };
        state.body.extend([
            S::Init(new_keys, empty(dictionary.keys)),
            S::Init(new_values, empty(dictionary.values)),
            S::Init(index, number(span, 0.0)),
            S::Init(count, length(local(span, keys, keys_ty))),
            S::While(
                binary(ast::BinaryOp::Lt, at(), local(span, count, Type::Float64)),
                vec![
                    S::Init(item, converted),
                    push(span, local(span, new_keys, dictionary.keys), key),
                    push(
                        span,
                        local(span, new_values, dictionary.values),
                        local(span, item, dictionary.value),
                    ),
                    increment(span, index),
                ],
            ),
            S::Init(
                output,
                Expr {
                    span,
                    ty: target,
                    kind: E::Record(vec![
                        (dictionaries::KEYS, local(span, new_keys, dictionary.keys)),
                        (
                            dictionaries::VALUES,
                            local(span, new_values, dictionary.values),
                        ),
                    ]),
                },
            ),
        ]);
        Ok(())
    }
}
