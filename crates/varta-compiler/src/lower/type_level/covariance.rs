//! Class-instance arrays shared under wider element types (v38
//! `type_level`, owner decision of 2026-11-06). TypeScript's arrays are
//! covariant: a `B[]` (or a `(B | C)[]`) is assignable to `A[]` when its
//! element classes extend `A`, and JavaScript keeps one array, so a push
//! through either name is seen through the other. Varta keeps that one
//! array. The covariance is unsound: an `A` stored through the `A[]` name
//! may later be read through the `B[]` name. Such a read tests the element
//! and throws `TypeError`, like the read of a lying type predicate.
//!
//! Lowering converts the array by a `Reinterpret` and records the (source,
//! target) array type pair. After every body is lowered, `resolve` joins the
//! pairs into components of array types that may name one array, and:
//!
//! - gives every array type of a component one element representation. An
//!   array of one class holds instance handles and an array of a union
//!   holds union cells; when a component has both, the unions' arrays hold
//!   the instances themselves (their reads box, their writes unbox), unless
//!   a union of the component admits `null` or `undefined`: then the class
//!   arrays hold union cells (their reads unbox, their writes box);
//! - tests every element read through an array type of the component whose
//!   element type does not admit every value stored through another type of
//!   the component: the descriptor of an instance, and `null`/`undefined`.
//!
//! Programs without such conversions keep their code.
use super::*;
use std::collections::BTreeSet;

/// The message of the `TypeError` an element read throws when the element
/// is not a value of the read's element type.
const ELEMENT_MISMATCH: &str = "array element is not a value of the array's element type";

/// The element values of an array type that may share its array with
/// another array type.
struct Element {
    /// The element class, or the class members of the element union.
    classes: Vec<u32>,
    /// The element union admits `null`.
    null: bool,
    /// The element union admits `undefined`.
    undefined: bool,
    /// The element type is a union.
    union: bool,
}

/// An array element type that converts to a wider element type without a
/// value conversion: a class, or a union of classes, `null` and `undefined`.
fn element(ty: Type, types: &ir::ValueTypes) -> Option<Element> {
    let mut element = Element {
        classes: Vec::new(),
        null: false,
        undefined: false,
        union: ty.is_union(),
    };
    if let Type::Class(class) = ty {
        element.classes.push(class);
        return Some(element);
    }
    if !element.union {
        return None;
    }
    for member in unions::members(ty, types) {
        match member {
            Type::Class(class) => element.classes.push(class),
            Type::Null => element.null = true,
            Type::Undefined => element.undefined = true,
            _ => return None,
        }
    }
    (!element.classes.is_empty()).then_some(element)
}

/// Whether an array of `given` elements may be shared as an array of
/// `wanted` elements: every class of `given` extends a class of `wanted`,
/// and `null`/`undefined` only enter a union that admits them.
pub(super) fn shares(given: Type, wanted: Type, types: &ir::ValueTypes) -> bool {
    let (Some(from), Some(into)) = (element(given, types), element(wanted, types)) else {
        return false;
    };
    (!from.null || into.null)
        && (!from.undefined || into.undefined)
        && from.classes.iter().all(|class| {
            into.classes
                .iter()
                .any(|target| types.classes.descends(*class, *target))
        })
}

/// Note that a value of array type `given` is viewed as array type `wanted`.
pub(super) fn note(given: u32, wanted: u32, types: &ir::ValueTypes) {
    types
        .type_level
        .covariance
        .borrow_mut()
        .insert((given, wanted));
}

/// The element representation of an array type whose component shares
/// arrays with another representation.
#[derive(Clone, Copy)]
enum Stored {
    /// A union-element array type that holds instances; its reads take
    /// them as this class before boxing.
    Instances(u32),
    /// A class-element array type that holds union cells of this union.
    Cells(Type),
}

/// The test of an element read: the descriptor shapes of the admitted
/// classes and whether `null` and `undefined` are admitted.
struct Check {
    shapes: Vec<u32>,
    null: bool,
    undefined: bool,
}

/// What `resolve` changes for the array types of the components.
#[derive(Default)]
struct Plan {
    stored: BTreeMap<u32, Stored>,
    checked: BTreeMap<u32, Check>,
}

/// Join the recorded conversions into components and rewrite every element
/// access of their array types (see the module documentation).
pub(in crate::lower) fn resolve(program: &mut ir::Program) -> Result<()> {
    let edges: Vec<(u32, u32)> = program
        .types
        .type_level
        .covariance
        .borrow()
        .iter()
        .copied()
        .collect();
    if edges.is_empty() {
        return Ok(());
    }
    let plan = plan(&edges, &mut program.types)?;
    let thrown = exceptions::exception_union(&mut program.types)?;
    let message = unions::intern(vec![Type::String, Type::Undefined], &mut program.types)?;
    let mut rewriter = Rewriter {
        types: &program.types,
        plan: &plan,
        thrown,
        message,
        locals: Vec::new(),
    };
    for function in &mut program.functions {
        rewriter.locals = std::mem::take(&mut function.locals);
        let rewritten = rewriter.statements(&mut function.body);
        function.locals = std::mem::take(&mut rewriter.locals);
        rewritten?;
    }
    for (id, stored) in &plan.stored {
        program.types.arrays[*id as usize] = match *stored {
            Stored::Instances(class) => Type::Class(class),
            Stored::Cells(union) => union,
        };
    }
    Ok(())
}

/// The components of `edges` with their representations and checks.
fn plan(edges: &[(u32, u32)], types: &mut ir::ValueTypes) -> Result<Plan> {
    let mut parent: BTreeMap<u32, u32> = BTreeMap::new();
    fn root(parent: &BTreeMap<u32, u32>, mut id: u32) -> u32 {
        while let Some(next) = parent.get(&id).filter(|next| **next != id) {
            id = *next;
        }
        id
    }
    for (given, wanted) in edges {
        parent.entry(*given).or_insert(*given);
        parent.entry(*wanted).or_insert(*wanted);
        let (a, b) = (root(&parent, *given), root(&parent, *wanted));
        if a != b {
            parent.insert(a, b);
        }
    }
    let mut components: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for id in parent.keys() {
        components.entry(root(&parent, *id)).or_default().push(*id);
    }
    let mut plan = Plan::default();
    for ids in components.values() {
        let elements: Vec<(u32, Element)> = ids
            .iter()
            .filter_map(|id| element(types.arrays[*id as usize], types).map(|e| (*id, e)))
            .collect();
        let direct = elements.iter().any(|(_, e)| !e.union);
        let unions = elements.iter().any(|(_, e)| e.union);
        let null = elements.iter().any(|(_, e)| e.null);
        let undefined = elements.iter().any(|(_, e)| e.undefined);
        let cells = null || undefined;
        let stored: BTreeSet<u32> = elements
            .iter()
            .flat_map(|(_, e)| e.classes.iter().copied())
            .collect();
        for (id, element) in &elements {
            if direct && unions {
                let representation = match (element.union, cells) {
                    (true, false) => Some(Stored::Instances(element.classes[0])),
                    (false, true) => Some(Stored::Cells(unions::intern(
                        vec![Type::Class(element.classes[0]), Type::Null],
                        types,
                    )?)),
                    _ => None,
                };
                if let Some(representation) = representation {
                    plan.stored.insert(*id, representation);
                }
            }
            let admitted = |class: &u32| {
                element
                    .classes
                    .iter()
                    .any(|target| types.classes.descends(*class, *target))
            };
            if !stored.iter().all(admitted)
                || null && !element.null
                || undefined && !element.undefined
            {
                let mut shapes: Vec<u32> = element
                    .classes
                    .iter()
                    .flat_map(|class| types.classes.family_shapes(*class))
                    .collect();
                shapes.sort_unstable();
                shapes.dedup();
                plan.checked.insert(
                    *id,
                    Check {
                        shapes,
                        null: element.null,
                        undefined: element.undefined,
                    },
                );
            }
        }
    }
    Ok(plan)
}

struct Rewriter<'a> {
    types: &'a ir::ValueTypes,
    plan: &'a Plan,
    /// The exception channel union and the `string | undefined` message type.
    thrown: Type,
    message: Type,
    /// The locals of the function being rewritten.
    locals: Vec<Type>,
}

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

/// The array type id of an operand.
fn array_id(value: &Expr) -> Option<u32> {
    match value.ty {
        Type::Array(id) => Some(id),
        _ => None,
    }
}

impl Rewriter<'_> {
    fn hidden(&mut self, ty: Type) -> u32 {
        self.locals.push(ty);
        self.locals.len() as u32 - 1
    }
    /// `throw new TypeError(…)` of a mismatching element read.
    fn mismatch(&self, span: swc_common::Span) -> Result<S> {
        let message = unions::widen(
            Expr {
                span,
                ty: Type::String,
                kind: E::String(self.types.profile.string_bytes(ELEMENT_MISMATCH)),
            },
            Some(self.message),
            self.types,
        )?;
        let class = varta_common::value::ErrorClass::TypeError;
        Ok(S::Throw(unions::widen(
            Expr {
                span,
                ty: Type::from_error_class(class),
                kind: E::NewError(class, Box::new(message)),
            },
            Some(self.thrown),
            self.types,
        )?))
    }
    /// `if (!test) throw new TypeError(…)`.
    fn unless(&self, test: Expr) -> Result<S> {
        let span = test.span;
        Ok(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Not(Box::new(test)),
            },
            vec![self.mismatch(span)?],
            Vec::new(),
        ))
    }
    /// `value`, evaluated once into a rooted local that `tests` (built from
    /// that local) examine before the value is the block's result.
    fn tested(
        &mut self,
        value: Expr,
        tests: impl FnOnce(&mut Self, Expr) -> Result<Vec<S>>,
    ) -> Result<Expr> {
        let (span, ty) = (value.span, value.ty);
        let saved = self.hidden(ty);
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let mut body = vec![S::Init(saved, value)];
        body.extend(tests(self, local(span, saved, ty))?);
        body.push(S::Init(output, local(span, saved, ty)));
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body,
            },
        })
    }
    /// The descriptor test of the instance `value` against `shapes`.
    fn instance_test(&mut self, value: Expr, shapes: &[u32]) -> Expr {
        let slot = self.hidden(Type::Number);
        Expr {
            span: value.span,
            ty: Type::Boolean,
            kind: E::IsClass {
                value: Box::new(value),
                shapes: shapes.to_vec(),
                slot,
            },
        }
    }
    /// `value` (an instance) when its descriptor is one of `check`'s shapes;
    /// otherwise a `TypeError` is thrown.
    fn checked_instance(&mut self, value: Expr, check: &Check) -> Result<Expr> {
        self.tested(value, |this, saved| {
            let test = this.instance_test(saved, &check.shapes);
            Ok(vec![this.unless(test)?])
        })
    }
    /// `value` (a union cell) when it holds `undefined` or `null` that
    /// `check` (or, for `undefined`, `missing`) admits, or an instance whose
    /// descriptor is one of `check`'s shapes; otherwise a `TypeError` is
    /// thrown.
    fn checked_cell(&mut self, value: Expr, check: &Check, missing: bool) -> Result<Expr> {
        let Some(class) = unions::members(value.ty, self.types)
            .into_iter()
            .find_map(|member| match member {
                Type::Class(class) => Some(class),
                _ => None,
            })
        else {
            return Ok(value);
        };
        self.tested(value, |this, saved| {
            let span = saved.span;
            let refuse = |admitted: bool, this: &Self| -> Result<Vec<S>> {
                Ok(if admitted {
                    Vec::new()
                } else {
                    vec![this.mismatch(span)?]
                })
            };
            let instance = Expr {
                span,
                ty: Type::Class(class),
                kind: E::UnboxUnion(Box::new(saved.clone())),
            };
            let test = this.instance_test(instance, &check.shapes);
            let present = vec![this.unless(test)?];
            let null = S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsNullish(Box::new(saved.clone())),
                },
                refuse(check.null, this)?,
                present,
            );
            Ok(vec![S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsUndefined(Box::new(saved)),
                },
                refuse(check.undefined || missing, this)?,
                vec![null],
            )])
        })
    }
    /// The element read `value` of array type `id`, tested when `id` is
    /// checked; `missing` admits the `undefined` of a missing element.
    fn checked_read(&mut self, value: Expr, id: u32, missing: bool) -> Result<Expr> {
        let Some(check) = self.plan.checked.get(&id) else {
            return Ok(value);
        };
        match value.ty {
            Type::Class(_) => self.checked_instance(value, check),
            ty if ty.is_cell() => self.checked_cell(value, check, missing),
            _ => Ok(value),
        }
    }
    /// An element stored into the array type `id`, in its representation:
    /// a cell is unboxed into an array of instances, an instance boxed into
    /// an array of cells.
    fn stored(&self, value: &mut Expr, id: u32) -> Result<()> {
        let Some(stored) = self.plan.stored.get(&id).copied() else {
            return Ok(());
        };
        let (ty, kind): (Type, fn(Box<Expr>) -> E) = match stored {
            Stored::Instances(_) if matches!(value.ty, Type::Class(_)) => return Ok(()),
            Stored::Instances(class) if value.ty.is_cell() => (Type::Class(class), E::UnboxUnion),
            Stored::Cells(_) if value.ty.is_cell() => return Ok(()),
            Stored::Cells(union) if matches!(value.ty, Type::Class(_)) => (union, E::BoxUnion),
            _ => {
                return Err(CompilerError::Internal(
                    "shared class array element is not a class instance or union cell".into(),
                ));
            }
        };
        let original = std::mem::replace(value, placeholder(value.span));
        *value = Expr {
            span: original.span,
            ty,
            kind: kind(Box::new(original)),
        };
        Ok(())
    }
    fn statements(&mut self, body: &mut [S]) -> Result<()> {
        body.iter_mut()
            .try_for_each(|statement| self.statement(statement))
    }
    /// Exhaustive on purpose: a new IR form must decide how it is traversed.
    fn statement(&mut self, statement: &mut S) -> Result<()> {
        match statement {
            S::Trap
            | S::Rebind(_)
            | S::RecordRest { .. }
            | S::BreakLabel(_)
            | S::ContinueLabel(_)
            | S::Break
            | S::Continue
            | S::Return(None) => Ok(()),
            S::Throw(value)
            | S::Init(_, value)
            | S::Set(_, value)
            | S::InitModuleConstant(_, value)
            | S::Expr(value)
            | S::Return(Some(value)) => self.expression(value),
            S::TryCatch { body, handler, .. } => {
                self.statements(body)?;
                self.statements(handler)
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                self.statements(body)?;
                self.statements(finalizer)
            }
            S::RecordEnumerate { values, .. } => values
                .iter_mut()
                .try_for_each(|value| self.expression(value)),
            S::SetField(object, _, value) => {
                self.expression(object)?;
                self.expression(value)
            }
            S::ArrayLength {
                array,
                length,
                error,
            } => [array, length, error]
                .into_iter()
                .try_for_each(|value| self.expression(value)),
            S::SetIndex {
                array,
                index,
                value,
                ..
            } => {
                self.expression(array)?;
                self.expression(index)?;
                self.expression(value)?;
                match array_id(array) {
                    Some(id) => self.stored(value, id),
                    None => Ok(()),
                }
            }
            S::Block(body) | S::Scope(_, body) | S::Label { body, .. } => self.statements(body),
            S::If(test, yes, no) => {
                self.expression(test)?;
                self.statements(yes)?;
                self.statements(no)
            }
            S::While(test, body) | S::DoWhile(test, body) => {
                self.expression(test)?;
                self.statements(body)
            }
            S::For(init, test, update, body) => {
                self.statements(init)?;
                if let Some(test) = test {
                    self.expression(test)?;
                }
                self.statements(update)?;
                self.statements(body)
            }
            S::Switch { value, cases, .. } => {
                self.expression(value)?;
                for (test, body) in cases {
                    if let Some(test) = test {
                        self.expression(test)?;
                    }
                    self.statements(body)?;
                }
                Ok(())
            }
        }
    }
    fn boxed(&mut self, values: &mut [&mut Box<Expr>]) -> Result<()> {
        values
            .iter_mut()
            .try_for_each(|value| self.expression(value))
    }
    /// Rewrite the children of `value`, then `value` itself.
    fn expression(&mut self, value: &mut Expr) -> Result<()> {
        self.children(value)?;
        self.element_access(value)
    }
    /// The element reads and writes of shared array types.
    fn element_access(&mut self, value: &mut Expr) -> Result<()> {
        let own = array_id(value);
        match &mut value.kind {
            E::Index(array, _) => {
                let Some(id) = array_id(array) else {
                    return Ok(());
                };
                let (physical, read) = match self.plan.stored.get(&id).copied() {
                    Some(Stored::Instances(class)) if value.ty.is_cell() => {
                        (Type::Class(class), E::BoxUnion as fn(Box<Expr>) -> E)
                    }
                    Some(Stored::Cells(union)) if matches!(value.ty, Type::Class(_)) => {
                        (union, E::UnboxUnion as fn(Box<Expr>) -> E)
                    }
                    _ => return self.replace_checked(value, id, false),
                };
                let ty = value.ty;
                let mut element = std::mem::replace(value, placeholder(value.span));
                element.ty = physical;
                let element = self.checked_read(element, id, false)?;
                *value = Expr {
                    span: element.span,
                    ty,
                    kind: read(Box::new(element)),
                };
                Ok(())
            }
            E::MaybeIndex(array, _) | E::ArrayAt(array, _) | E::ArrayPop(array) => {
                match array_id(array) {
                    Some(id) => self.replace_checked(value, id, true),
                    None => Ok(()),
                }
            }
            E::Array(elements) => {
                let Some(id) = own else {
                    return Ok(());
                };
                for element in elements.iter_mut().flatten() {
                    self.stored(element, id)?;
                }
                Ok(())
            }
            E::ArrayPush(array, items) => {
                let Some(id) = array_id(array) else {
                    return Ok(());
                };
                if self.plan.stored.contains_key(&id)
                    && items.ty != array.ty
                    && let E::Array(elements) = &mut items.kind
                {
                    for element in elements.iter_mut().flatten() {
                        self.stored(element, id)?;
                    }
                    items.ty = array.ty;
                }
                Ok(())
            }
            E::ArrayFill(array, item, _, _) | E::ArraySearch(_, array, item, _) => {
                match array_id(array) {
                    Some(id) => self.stored(item, id),
                    None => Ok(()),
                }
            }
            // A `Map`/`Set` keyed by a class whose key vector holds union
            // cells (the class array type shares arrays with a union array
            // admitting `null`/`undefined`): its probes read each entry's
            // payload (`KeyHash::Cell`).
            E::Intrinsic(ir::Intrinsic::Collection(_, hash), args)
                if *hash == ir::KeyHash::Identity
                    && args.first().and_then(array_id).is_some_and(|id| {
                        matches!(self.plan.stored.get(&id), Some(Stored::Cells(_)))
                    }) =>
            {
                *hash = ir::KeyHash::Cell;
                Ok(())
            }
            E::Intrinsic(_, args) => {
                if args
                    .iter()
                    .filter_map(array_id)
                    .any(|id| self.plan.stored.contains_key(&id))
                {
                    return Err(CompilerError::Internal(
                        "a probe of a key vector with a changed element representation".into(),
                    ));
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
    /// Replace the element read `value` of array type `id` by its test.
    fn replace_checked(&mut self, value: &mut Expr, id: u32, missing: bool) -> Result<()> {
        if !self.plan.checked.contains_key(&id) {
            return Ok(());
        }
        let read = std::mem::replace(value, placeholder(value.span));
        *value = self.checked_read(read, id, missing)?;
        Ok(())
    }
    /// Exhaustive on purpose: a new IR form must decide how it is traversed.
    fn children(&mut self, value: &mut Expr) -> Result<()> {
        match &mut value.kind {
            E::Integer(_)
            | E::Float64(_)
            | E::String(_)
            | E::Local(_)
            | E::Status
            | E::ModuleConstant(_)
            | E::Zero
            | E::FunctionRef { .. } => Ok(()),
            E::Assignment { value, body, .. } => {
                self.expression(value)?;
                self.statements(body)
            }
            E::Await {
                promise, result, ..
            } => self.boxed(&mut [promise, result]),
            E::Schedule { args, .. } => {
                args.iter_mut().try_for_each(|value| self.expression(value))
            }
            E::ValueBlock { body, .. } => self.statements(body),
            E::GeneratorStep {
                generator, value, ..
            } => self.boxed(&mut [generator, value]),
            E::FunctionCheck { value, .. }
            | E::GeneratorValue(value)
            | E::Reinterpret(value)
            | E::Suspend { value, .. }
            | E::StringTrim { value, .. }
            | E::TextUnits { value, .. }
            | E::ArrayEnumerate { value, .. }
            | E::ArrayReserve { length: value, .. }
            | E::NewError(_, value)
            | E::ErrorName(value)
            | E::ErrorMessage(value)
            | E::ErrorText(value)
            | E::ErrorOwn(value)
            | E::BoxUnion(value)
            | E::UnboxUnion(value)
            | E::IsUndefined(value)
            | E::IsError(value)
            | E::IsClass { value, .. }
            | E::ArrayKind(value, _)
            | E::IsNullish(value)
            | E::Undefined(value)
            | E::Negative(value)
            | E::ToBoolean(value)
            | E::Not(value)
            | E::Capture(value)
            | E::ResultField(_, value)
            | E::HasOwn(value, _)
            | E::Field(value, _)
            | E::Json(value, _)
            | E::ParseJson(value)
            | E::KvStage(_, value)
            | E::KvDecode(_, value)
            | E::ArrayPop(value)
            | E::ArrayReverse(value)
            | E::IndexKey(value)
            | E::IsUnionMember(value, _)
            | E::Length(value) => self.expression(value),
            E::FunctionCall {
                callee,
                ignored_receiver,
                args,
                ..
            } => {
                self.expression(callee)?;
                if let Some(receiver) = ignored_receiver {
                    self.expression(receiver)?;
                }
                args.iter_mut().try_for_each(|value| self.expression(value))
            }
            E::StringSearch(_, a, b, c)
            | E::Conditional(a, b, c)
            | E::ArraySearch(_, a, b, c)
            | E::ArraySlice(a, b, c) => self.boxed(&mut [a, b, c]),
            E::StringRange {
                value, start, end, ..
            } => self.boxed(&mut [value, start, end]),
            E::StringRepeat {
                value,
                count,
                error,
                ..
            } => self.boxed(&mut [value, count, error]),
            E::UnionLogical(_, a, b)
            | E::Sequence(a, b)
            | E::Binary(_, a, b)
            | E::NumberSameValue(a, b)
            | E::MaybeIndex(a, b)
            | E::ArrayHasOwn(a, b)
            | E::ArrayStringHasOwn(a, b)
            | E::ArrayPush(a, b)
            | E::ArrayAt(a, b)
            | E::Index(a, b) => self.boxed(&mut [a, b]),
            E::RecordDelete { object, absent, .. } => self.boxed(&mut [object, absent]),
            E::ArrayFill(a, b, c, d) => self.boxed(&mut [a, b, c, d]),
            E::Math(_, args)
            | E::NumberPredicate(_, args)
            | E::Intrinsic(_, args)
            | E::RegExp(_, args)
            | E::Call(_, args)
            | E::Host(_, args)
            | E::CheckedBigIntHost { args, .. } => {
                args.iter_mut().try_for_each(|value| self.expression(value))
            }
            E::Record(fields) | E::OptionalRecord(fields, _) => fields
                .iter_mut()
                .try_for_each(|(_, value)| self.expression(value)),
            E::Array(elements) => elements
                .iter_mut()
                .flatten()
                .try_for_each(|value| self.expression(value)),
            E::ArrayDelete {
                array,
                index,
                error,
            } => {
                self.expression(array)?;
                if let Some(index) = index {
                    self.expression(index)?;
                }
                if let Some(error) = error {
                    self.expression(error)?;
                }
                Ok(())
            }
        }
    }
}

/// A value-less expression that holds a node's place while it is rebuilt.
fn placeholder(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Void,
        kind: E::Zero,
    }
}
