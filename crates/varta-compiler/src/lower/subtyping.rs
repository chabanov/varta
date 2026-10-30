//! Structural width subtyping (v38 `structural_subtyping`).
//!
//! A value of a record shape or class instance type with more members is
//! assignable to a record shape (an interface, alias or object type) whose
//! members it provides with assignable types; optional members may be
//! missing. The value is not copied: the same managed object is viewed
//! through the narrower type, so identity, mutation and `JSON.stringify`
//! see the object itself, as in JavaScript.
//!
//! Lowering records each admitted coercion as an edge from the source type to
//! the target shape (`Views`). After every body is lowered, `resolve` closes
//! the edges transitively and rewrites each member access whose static type
//! is a shape that received a wider value (a *view*): reads, writes, `in`
//! tests and function-field calls go through one synthesized accessor per
//! (view, member, operation), which selects the object's layout by its
//! descriptor and accesses the source's own slot (or calls a class getter or
//! method). Shapes that never receive a wider value keep their direct slot
//! accesses, so programs without coercions keep their code.
use super::*;
use std::cell::RefCell;
use std::collections::BTreeSet;

/// The coercions admitted by lowering and, after `resolve`, the foreign
/// source types of every view shape.
#[derive(Default)]
pub(crate) struct Views {
    /// (source record shape or class instance type, target record shape).
    edges: RefCell<BTreeSet<(Type, u32)>>,
    /// After `resolve`: the source types whose objects a value of the view
    /// shape may hold besides its own layouts, in descriptor-test order.
    sources: BTreeMap<u32, Vec<Type>>,
    /// `"name" in value` for a name record `shape` does not declare: the
    /// reserved function `(value) -> boolean`, by (shape, name). Its body is
    /// `false` unless `resolve` finds sources that declare the name.
    names: BTreeMap<(u32, String), usize>,
    /// Closed record shapes whose values entered an `unknown` cell, with the
    /// record shapes they reach (fields, elements, members).
    boxed: RefCell<BTreeSet<u32>>,
    /// The reserved `(record) -> Record<string, unknown>` function that
    /// reads a closed record held by an `unknown` cell as a JSON object, and
    /// that dictionary type; `resolve` adds one branch per boxed shape.
    graph: Option<(usize, Type)>,
}
impl Views {
    /// Note the closed record shapes a value of `ty` entering an `unknown`
    /// cell may hold (v38 JSON-object reads of records).
    pub(crate) fn note_boxed(&self, ty: Type, types: &ir::ValueTypes) {
        let mut stack = vec![ty];
        let mut seen = BTreeSet::new();
        while let Some(ty) = stack.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match ty {
                Type::Record(id) if types.records[id as usize].class.is_none() => {
                    self.boxed.borrow_mut().insert(id);
                    stack.extend(types.records[id as usize].fields.iter().map(|f| f.ty));
                }
                Type::Array(id) => stack.push(types.arrays[id as usize]),
                Type::Union(id) | Type::ReferenceUnion(id) => {
                    stack.extend(types.unions[id as usize].iter().copied());
                }
                Type::Dictionary(id) => stack.push(types.dictionaries[id as usize].value),
                _ => {}
            }
        }
    }
}
impl Views {
    /// The record shapes whose layouts a value of record `shape` may have:
    /// its own and those of every source of the view (class families
    /// included). Used by union member tests (`emit::shared_members`).
    pub(crate) fn shapes(&self, shape: u32, types: &ir::ValueTypes) -> Vec<u32> {
        let mut shapes = vec![shape];
        for source in self.sources.get(&shape).into_iter().flatten() {
            match *source {
                Type::Record(id) => shapes.push(id),
                Type::Class(class) => shapes.extend(types.classes.family_shapes(class)),
                _ => {}
            }
        }
        shapes.sort_unstable();
        shapes.dedup();
        shapes
    }
}

/// One structural check: assumed pairs (cyclic shapes are checked
/// coinductively) and the nested edges it needs, committed only on success.
struct Check<'a> {
    types: &'a ir::ValueTypes,
    assumed: BTreeSet<(Type, u32)>,
    edges: Vec<(Type, u32)>,
}
impl<'a> Check<'a> {
    fn new(types: &'a ir::ValueTypes) -> Self {
        Self {
            types,
            assumed: BTreeSet::new(),
            edges: Vec::new(),
        }
    }
    /// Whether a value of `actual` (a record shape or class instance type)
    /// may be viewed as record `target`; otherwise the reasons.
    fn object(&mut self, actual: Type, target: u32) -> std::result::Result<(), Vec<String>> {
        if actual == Type::Record(target)
            || self.assumed.contains(&(actual, target))
            || self
                .types
                .subtyping
                .edges
                .borrow()
                .contains(&(actual, target))
        {
            return Ok(());
        }
        let types = self.types;
        let shape = &types.records[target as usize];
        if shape.class.is_some() {
            return Err(Vec::new());
        }
        let mark = self.edges.len();
        self.assumed.insert((actual, target));
        let mut reasons = Vec::new();
        match actual {
            Type::Record(source) if types.records[source as usize].class.is_none() => {
                let fields = &types.records[source as usize].fields;
                for wanted in &shape.fields {
                    match fields.iter().find(|field| field.name == wanted.name) {
                        None if wanted.optional => {}
                        None => reasons.push(format!("missing field {}", wanted.name)),
                        Some(field) if field.optional && !wanted.optional => reasons.push(format!(
                            "field {} is optional in the source but required in the target",
                            wanted.name
                        )),
                        Some(field) => {
                            if !self.convertible(field.ty, wanted.ty) {
                                reasons.push(format!(
                                    "field {}: expected {}, found {}",
                                    wanted.name,
                                    describe(wanted.ty, types),
                                    describe(field.ty, types)
                                ));
                            }
                        }
                    }
                }
            }
            Type::Class(class) if !types.classes.get(class).opaque => {
                for wanted in &shape.fields {
                    if let Err(reason) = self.member(class, wanted) {
                        reasons.push(reason);
                    }
                }
            }
            _ => reasons.push(format!("{} is not an object type", describe(actual, types))),
        }
        self.assumed.remove(&(actual, target));
        if reasons.is_empty() {
            self.edges.push((actual, target));
            Ok(())
        } else {
            self.edges.truncate(mark);
            Err(reasons)
        }
    }
    /// Whether class `class` provides the view member `wanted`.
    fn member(&mut self, class: u32, wanted: &ir::RecordField) -> std::result::Result<(), String> {
        let types = self.types;
        let name = &wanted.name;
        let mismatch = |found: Type| {
            format!(
                "member {name}: expected {}, found {}",
                describe(wanted.ty, types),
                describe(found, types)
            )
        };
        if let Some((_, field)) = types.classes.field(class, name) {
            if field.access != classes::Access::Public {
                return Err(format!("member {name} is not public in the class"));
            }
            let ty = field.ty;
            return if self.convertible(ty, wanted.ty) {
                Ok(())
            } else {
                Err(mismatch(ty))
            };
        }
        let info = types.classes.get(class);
        if let Some(method) = info.methods.get(name) {
            if method.access != classes::Access::Public {
                return Err(format!("member {name} is not public in the class"));
            }
            let signature = &method.signature;
            if method.getter {
                return if self.convertible(signature.result, wanted.ty) {
                    Ok(())
                } else {
                    Err(mismatch(signature.result))
                };
            }
            // A class method is called directly through the view; it has no
            // function value, so the member must be a required function.
            let Type::Function(function) = wanted.ty else {
                return Err(format!(
                    "method {name} cannot provide the member {}; class methods are called, \
                     never read as function values",
                    describe(wanted.ty, types)
                ));
            };
            let wanted = &types.functions[function as usize];
            let matches = wanted.receiver.is_none()
                && wanted.shape == signature.shape
                && wanted.params == signature.params
                && (wanted.result == Type::Void
                    || self.convertible(signature.result, wanted.result));
            return if matches {
                Ok(())
            } else {
                Err(format!(
                    "method {name} does not match {}",
                    describe(Type::Function(function), types)
                ))
            };
        }
        if info.generic_methods.contains_key(name) {
            return Err(format!(
                "generic method {name} cannot provide the non-generic member {}",
                describe(wanted.ty, types)
            ));
        }
        if wanted.optional {
            Ok(())
        } else {
            Err(format!("missing member {name}"))
        }
    }
    /// Whether a value of `actual` converts to `expected` without a copy:
    /// the nominal rules, or a view of a record shape, or a union whose
    /// members each convert to a member of the expected union.
    fn convertible(&mut self, actual: Type, expected: Type) -> bool {
        let types = self.types;
        if arguments::nominal_accepts(expected, actual, types) {
            return true;
        }
        match expected {
            Type::Record(target) if matches!(actual, Type::Record(_) | Type::Class(_)) => {
                self.object(actual, target).is_ok()
            }
            union if union.is_union() => {
                let target = unions::members(union, types);
                let mark = self.edges.len();
                let all = unions::members(actual, types).into_iter().all(|member| {
                    literals::member_of(member, &target, types)
                        || target
                            .iter()
                            .any(|t| arguments::nominal_accepts(*t, member, types))
                        || matches!(member, Type::Record(_) | Type::Class(_))
                            && target.iter().any(
                                |t| matches!(t, Type::Record(r) if self.object(member, *r).is_ok()),
                            )
                });
                if !all {
                    self.edges.truncate(mark);
                }
                all
            }
            _ => false,
        }
    }
    fn commit(self) {
        self.types.subtyping.edges.borrow_mut().extend(self.edges);
    }
}

/// `ty` for a diagnostic: a named interface or alias by its name, a record
/// shape by its members with their types.
pub(super) fn describe(ty: Type, types: &ir::ValueTypes) -> String {
    if let Some(name) = named::name_of(types, ty) {
        return name;
    }
    match ty {
        Type::Record(id) if types.records[id as usize].class.is_none() => {
            let fields: Vec<String> = types.records[id as usize]
                .fields
                .iter()
                .map(|field| {
                    let ty = if field.optional {
                        // The stored type includes `undefined`; show the declared one.
                        unions::members(field.ty, types)
                            .into_iter()
                            .filter(|member| *member != Type::Undefined)
                            .map(|member| shallow(member, types))
                            .collect::<Vec<_>>()
                            .join(" | ")
                    } else {
                        shallow(field.ty, types)
                    };
                    format!(
                        "{}{}: {ty}",
                        field.name,
                        if field.optional { "?" } else { "" }
                    )
                })
                .collect();
            format!("{{ {} }}", fields.join("; "))
        }
        Type::Class(class) => types.classes.get(class).name.clone(),
        Type::Array(id) => format!("{}[]", shallow(types.arrays[id as usize], types)),
        _ => void_functions::describe(ty, types),
    }
}
/// A member type inside a described record: named types by name, other
/// records abbreviated by their member names.
fn shallow(ty: Type, types: &ir::ValueTypes) -> String {
    if let Some(name) = named::name_of(types, ty) {
        return name;
    }
    match ty {
        Type::Class(class) => types.classes.get(class).name.clone(),
        // A discriminant by its values (`type: 'x'`).
        Type::Literal(_, id) => literals::describe(id, types),
        _ => void_functions::describe(ty, types),
    }
}

/// Whether a value of `actual` is assignable to `expected` through a view
/// (no edge is committed).
pub(super) fn assignable(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    if !types.profile.structural_subtyping() || expected == actual {
        return false;
    }
    Check::new(types).convertible(actual, expected)
}

/// The reasons a value of `actual` is not assignable to the object type
/// `expected`, for a mismatch diagnostic; `None` when either is not an
/// object type.
pub(super) fn reasons(expected: Type, actual: Type, types: &ir::ValueTypes) -> Option<String> {
    if let (Type::Array(wanted), Type::Array(given)) = (expected, actual) {
        let (wanted, given) = (types.arrays[wanted as usize], types.arrays[given as usize]);
        return assignable(wanted, given, types).then(|| {
            "array element types must be equal: an array is mutable, so its elements are not \
             viewed through a wider element type; copy it, e.g. with map"
                .to_owned()
        });
    }
    let Type::Record(target) = expected else {
        return None;
    };
    if !matches!(actual, Type::Record(_) | Type::Class(_)) {
        return None;
    }
    let reasons = Check::new(types).object(actual, target).err()?;
    let shape = &types.records[target as usize];
    let extra: Vec<&str> = match actual {
        Type::Record(id) => types.records[id as usize]
            .fields
            .iter()
            .filter(|field| !shape.fields.iter().any(|f| f.name == field.name))
            .map(|field| field.name.as_str())
            .collect(),
        _ => Vec::new(),
    };
    let mut text = reasons.join("; ");
    if !extra.is_empty() {
        text.push_str(&format!(
            "{}extra fields (allowed): {}",
            if text.is_empty() { "" } else { "; " },
            extra.join(", ")
        ));
    }
    Some(text)
}

/// Convert `value` to `expected` through a view when it is a record shape or
/// class instance (or a union of them) that is assignable to the record
/// shape `expected` or to a record member of the union `expected`. Any other
/// value is returned unchanged for the nominal rules.
pub(super) fn coerce(value: Expr, expected: Option<Type>, types: &ir::ValueTypes) -> Result<Expr> {
    let Some(expected) = expected else {
        return Ok(value);
    };
    if !types.profile.structural_subtyping() || value.ty == expected {
        return Ok(value);
    }
    let object = |ty: Type| matches!(ty, Type::Record(_) | Type::Class(_));
    match expected {
        Type::Record(target) if object(value.ty) => {
            let mut check = Check::new(types);
            if check.object(value.ty, target).is_ok() {
                check.commit();
                return Ok(viewed(value, expected));
            }
            Ok(value)
        }
        union if union.is_union() => {
            let target = unions::members(union, types);
            let records: Vec<u32> = target
                .iter()
                .filter_map(|t| match t {
                    Type::Record(r) => Some(*r),
                    _ => None,
                })
                .collect();
            if records.is_empty() {
                return Ok(value);
            }
            let actual = unions::members(value.ty, types);
            if actual
                .iter()
                .all(|m| literals::member_of(*m, &target, types))
            {
                return Ok(value);
            }
            let mut check = Check::new(types);
            let mut mapped = None;
            for member in &actual {
                if literals::member_of(*member, &target, types)
                    || target
                        .iter()
                        .any(|t| arguments::nominal_accepts(*t, *member, types))
                {
                    continue;
                }
                if !object(*member) {
                    return Ok(value);
                }
                // Every record member the object is assignable to views it,
                // so narrowing to any of them reads it through its layout.
                let viewing: Vec<u32> = records
                    .iter()
                    .copied()
                    .filter(|r| check.object(*member, *r).is_ok())
                    .collect();
                let Some(first) = viewing.first().copied() else {
                    return Ok(value);
                };
                overlap(*member, &viewing, &records, types)?;
                mapped = Some(first);
            }
            check.commit();
            Ok(if value.ty.is_union() {
                // The cell's record tag already holds the viewed object.
                viewed(value, expected)
            } else {
                match mapped {
                    Some(record) => viewed(value, Type::Record(record)),
                    None => value,
                }
            })
        }
        _ => Ok(value),
    }
}

/// The same handle as type `ty`. A local read is retyped in place (flow
/// analyses read IR locals); any other expression keeps its own type, which
/// may fix its layout (a record literal), under a `Reinterpret`.
fn viewed(value: Expr, ty: Type) -> Expr {
    if matches!(value.kind, E::Local(_)) {
        return Expr { ty, ..value };
    }
    Expr {
        span: value.span,
        ty,
        kind: E::Reinterpret(Box::new(value)),
    }
}

/// The member names of a record shape or class instance type.
fn member_names(ty: Type, types: &ir::ValueTypes) -> Vec<String> {
    match ty {
        Type::Record(id) => types.records[id as usize]
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect(),
        Type::Class(class) => {
            let info = types.classes.get(class);
            info.fields
                .iter()
                .map(|field| field.name.clone())
                .chain(info.methods.keys().cloned())
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Reject an object entering a union of record shapes when a key it has
/// is declared only by members it is not assignable to: an `in` test of that
/// key would narrow the union to members that cannot view the object.
fn overlap(member: Type, viewing: &[u32], records: &[u32], types: &ir::ValueTypes) -> Result<()> {
    let declares = |record: u32, name: &str| {
        types.records[record as usize]
            .fields
            .iter()
            .any(|f| f.name == name)
    };
    for name in member_names(member, types) {
        if viewing.iter().any(|r| declares(*r, &name)) {
            continue;
        }
        if let Some(other) = records
            .iter()
            .find(|r| !viewing.contains(r) && declares(**r, &name))
        {
            return Err(type_error(format!(
                "{} cannot enter this union: it has {name}, which only {} declares, but it is \
                 not assignable to it; an `in` test of {name} could not view it",
                describe(member, types),
                describe(Type::Record(*other), types)
            )));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Access {
    Get(Type),
    Set(Type),
    Has,
    Call(Type),
}

/// Close the admitted edges and route every member access of a view shape
/// through its accessors (see the module documentation).
pub(super) fn resolve(program: &mut ir::Program) -> Result<()> {
    if !program.types.profile.structural_subtyping() {
        return Ok(());
    }
    let edges = program.types.subtyping.edges.borrow().clone();
    if edges.is_empty() && program.types.subtyping.graph.is_none() {
        return Ok(());
    }
    let mut sources: BTreeMap<u32, Vec<Type>> = BTreeMap::new();
    for target in edges.iter().map(|(_, target)| *target) {
        if sources.contains_key(&target) {
            continue;
        }
        let mut found = Vec::new();
        let mut visited = BTreeSet::from([target]);
        let mut stack = vec![target];
        while let Some(shape) = stack.pop() {
            for (source, _) in edges.iter().filter(|(_, t)| *t == shape) {
                match *source {
                    Type::Record(id) => {
                        if visited.insert(id) {
                            found.push(*source);
                            stack.push(id);
                        }
                    }
                    other => {
                        if !found.contains(&other) {
                            found.push(other);
                        }
                    }
                }
            }
        }
        sources.insert(target, found);
    }
    program.types.subtyping.sources = sources;
    let count = program.functions.len();
    let mut rewriter = Rewriter {
        types: &program.types,
        accessors: BTreeMap::new(),
        created: Vec::new(),
        next: count,
        source: None,
        span: swc_common::DUMMY_SP,
        locals: Vec::new(),
    };
    for function in &mut program.functions {
        rewriter.source = Some(function.source.clone());
        rewriter.span = function.span;
        rewriter.locals = std::mem::take(&mut function.locals);
        let rewritten = rewriter.statements(&mut function.body);
        function.locals = std::mem::take(&mut rewriter.locals);
        rewritten?;
    }
    let names: Vec<((u32, String), usize)> = program
        .types
        .subtyping
        .names
        .iter()
        .map(|(key, index)| (key.clone(), *index))
        .collect();
    for ((shape, name), index) in names {
        if program.types.subtyping.sources.contains_key(&shape) {
            program.functions[index].body = rewriter.has_name(shape, &name);
        }
    }
    if let Some((index, graph)) = program.types.subtyping.graph {
        let dispatch = rewriter.graph_branches(graph)?;
        program.functions[index].body.splice(0..0, dispatch);
    }
    let created = std::mem::take(&mut rewriter.created);
    if program.types.subtyping.edges.borrow().len() != edges.len() {
        return Err(CompilerError::Internal(
            "structural view admitted after resolution".into(),
        ));
    }
    program.functions.extend(created);
    Ok(())
}

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn constant(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}

struct Rewriter<'a> {
    types: &'a ir::ValueTypes,
    accessors: BTreeMap<(u32, u32, Access), usize>,
    created: Vec<ir::Function>,
    next: usize,
    source: Option<Source>,
    /// The span and locals of the function being rewritten.
    span: swc_common::Span,
    locals: Vec<Type>,
}
impl Rewriter<'_> {
    fn view(&self, ty: Type) -> Option<u32> {
        match ty {
            Type::Record(shape) if self.types.subtyping.sources.contains_key(&shape) => Some(shape),
            _ => None,
        }
    }
    fn locate(&self, error: CompilerError, span: swc_common::Span) -> CompilerError {
        match &self.source {
            Some(source) => source.locate(error, span),
            None => error,
        }
    }
    fn statements(&mut self, body: &mut [S]) -> Result<()> {
        for statement in body {
            self.statement(statement)?;
        }
        Ok(())
    }
    fn statement(&mut self, statement: &mut S) -> Result<()> {
        match statement {
            S::Trap
            | S::Rebind(_)
            | S::BreakLabel(_)
            | S::ContinueLabel(_)
            | S::Break
            | S::Continue => {}
            S::RecordRest { from, .. } => {
                if self.view(Type::Record(*from)).is_some() {
                    return Err(self.locate(
                        unsupported(&format!(
                            "rest pattern over {}, which a wider object type is assigned to",
                            describe(Type::Record(*from), self.types)
                        )),
                        self.span,
                    ));
                }
            }
            S::Throw(value)
            | S::Init(_, value)
            | S::Set(_, value)
            | S::InitModuleConstant(_, value)
            | S::Expr(value) => self.expression(value)?,
            S::Return(value) => {
                if let Some(value) = value {
                    self.expression(value)?;
                }
            }
            S::TryCatch { body, handler, .. } => {
                self.statements(body)?;
                self.statements(handler)?;
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                self.statements(body)?;
                self.statements(finalizer)?;
            }
            S::RecordEnumerate { values, .. } => {
                for value in values.iter_mut() {
                    self.expression(value)?;
                }
                if let S::RecordEnumerate {
                    source,
                    target,
                    shape,
                    values,
                    ..
                } = statement
                    && let Some(view) = self.view(Type::Record(*shape))
                {
                    // An object of the view's own layouts keeps its exact
                    // key order; another source's object enumerates the
                    // view's members.
                    let (source, target, values) = (*source, *target, values.clone());
                    let original = statement.clone();
                    let span = self.span;
                    let descriptor = self.locals.len() as u32;
                    self.locals.push(Type::Number);
                    let viewed = self.view_enumeration(view, source, target, values)?;
                    *statement = S::If(
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::IsClass {
                                value: Box::new(local(span, source, Type::Record(view))),
                                shapes: vec![view],
                                slot: descriptor,
                            },
                        },
                        vec![original],
                        vec![viewed],
                    );
                }
            }
            S::SetField(object, slot, value) => {
                self.expression(object)?;
                self.expression(value)?;
                if let Some(shape) = self.view(object.ty) {
                    let span = object.span;
                    let index = self.accessor(shape, *slot, Access::Set(value.ty), span)?;
                    let object = std::mem::replace(object, constant(span, false));
                    let value = std::mem::replace(value, constant(span, false));
                    *statement = S::Expr(Expr {
                        span,
                        ty: Type::Void,
                        kind: E::Call(index, vec![object, value]),
                    });
                }
            }
            S::ArrayLength {
                array,
                length,
                error,
            } => {
                self.expression(array)?;
                self.expression(length)?;
                self.expression(error)?;
            }
            S::SetIndex {
                array,
                index,
                value,
                ..
            } => {
                self.expression(array)?;
                self.expression(index)?;
                self.expression(value)?;
            }
            S::Block(body) | S::Scope(_, body) | S::Label { body, .. } => self.statements(body)?,
            S::If(test, yes, no) => {
                self.expression(test)?;
                self.statements(yes)?;
                self.statements(no)?;
            }
            S::While(test, body) | S::DoWhile(test, body) => {
                self.expression(test)?;
                self.statements(body)?;
            }
            S::For(init, test, update, body) => {
                self.statements(init)?;
                if let Some(test) = test {
                    self.expression(test)?;
                }
                self.statements(update)?;
                self.statements(body)?;
            }
            S::Switch { value, cases, .. } => {
                self.expression(value)?;
                for (test, body) in cases {
                    if let Some(test) = test {
                        self.expression(test)?;
                    }
                    self.statements(body)?;
                }
            }
        }
        Ok(())
    }
    /// The present-entry compaction of `Object.keys`/`values`/`entries` (and
    /// dictionary copies) of a view holding another source's object: the
    /// entries of the view's present members, in its canonical member order
    /// (the object's own key order is not visible through the view).
    fn view_enumeration(
        &mut self,
        view: u32,
        source: u32,
        target: u32,
        values: Vec<Expr>,
    ) -> Result<S> {
        let span = self.span;
        let array = self.locals[target as usize];
        let fresh = self.locals.len() as u32;
        self.locals.push(array);
        let record = Type::Record(view);
        let mut body = vec![S::Init(
            fresh,
            Expr {
                span,
                ty: array,
                kind: E::Array(Vec::new()),
            },
        )];
        let fields = self.types.records[view as usize].fields.clone();
        for (slot, (field, value)) in fields.iter().zip(values).enumerate() {
            let push = S::Expr(Expr {
                span,
                ty: self.types.number(),
                kind: E::ArrayPush(
                    Box::new(local(span, fresh, array)),
                    Box::new(Expr {
                        span,
                        ty: array,
                        kind: E::Array(vec![Some(value)]),
                    }),
                ),
            });
            if field.optional {
                let index = self.accessor(view, slot as u32, Access::Has, span)?;
                body.push(S::If(
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Call(index, vec![local(span, source, record)]),
                    },
                    vec![push],
                    Vec::new(),
                ));
            } else {
                body.push(push);
            }
        }
        body.push(S::Set(target, local(span, fresh, array)));
        Ok(S::Block(body))
    }
    fn boxed(&mut self, values: &mut [&mut Box<Expr>]) -> Result<()> {
        for value in values {
            self.expression(value)?;
        }
        Ok(())
    }
    /// Whether some source of view `shape` provides member `slot` by a class
    /// method, which only a call can reach.
    fn method_backed(&self, shape: u32, slot: u32) -> bool {
        let name = &self.types.records[shape as usize].fields[slot as usize].name;
        self.types.subtyping.sources[&shape]
            .iter()
            .any(|source| match *source {
                Type::Class(class) => {
                    self.types.classes.field(class, name).is_none()
                        && self
                            .types
                            .classes
                            .get(class)
                            .methods
                            .get(name)
                            .is_some_and(|method| !method.getter)
                }
                _ => false,
            })
    }
    fn expression(&mut self, value: &mut Expr) -> Result<()> {
        // A function-field call through a view that a class method provides
        // calls the method directly: the arguments are evaluated before the
        // member is selected.
        if let E::FunctionCall {
            callee,
            ignored_receiver: None,
            ..
        } = &value.kind
            && let E::Field(base, slot) = &callee.kind
            && let Some(shape) = self.view(base.ty)
            && self.method_backed(shape, *slot)
        {
            let slot = *slot;
            let function = callee.ty;
            let span = value.span;
            let E::FunctionCall { callee, args, .. } = std::mem::replace(&mut value.kind, E::Zero)
            else {
                unreachable!("matched above");
            };
            let E::Field(mut base, _) = callee.kind else {
                unreachable!("matched above");
            };
            self.expression(&mut base)?;
            let mut operands = vec![*base];
            for mut arg in args {
                self.expression(&mut arg)?;
                operands.push(arg);
            }
            let index = self.accessor(shape, slot, Access::Call(function), span)?;
            value.kind = E::Call(index, operands);
            return Ok(());
        }
        match &mut value.kind {
            E::Integer(_)
            | E::Float64(_)
            | E::String(_)
            | E::Local(_)
            | E::Status
            | E::ModuleConstant(_)
            | E::Zero
            | E::FunctionRef { .. } => {}
            E::Assignment { value, body, .. } => {
                self.expression(value)?;
                self.statements(body)?;
            }
            E::Await {
                promise, result, ..
            } => self.boxed(&mut [promise, result])?,
            E::Schedule { args, .. } => {
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::ValueBlock { body, .. } => self.statements(body)?,
            E::GeneratorStep {
                generator, value, ..
            } => self.boxed(&mut [generator, value])?,
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
            | E::ArrayPop(value)
            | E::ArrayReverse(value)
            | E::IndexKey(value)
            | E::IsUnionMember(value, _)
            | E::Length(value) => self.expression(value)?,
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
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::StringSearch(_, a, b, c)
            | E::Conditional(a, b, c)
            | E::ArraySearch(_, a, b, c)
            | E::ArraySlice(a, b, c) => self.boxed(&mut [a, b, c])?,
            E::StringRange {
                value, start, end, ..
            } => self.boxed(&mut [value, start, end])?,
            E::StringRepeat {
                value,
                count,
                error,
                ..
            } => self.boxed(&mut [value, count, error])?,
            E::UnionLogical(_, a, b)
            | E::Sequence(a, b)
            | E::Binary(_, a, b)
            | E::NumberSameValue(a, b)
            | E::MaybeIndex(a, b)
            | E::ArrayHasOwn(a, b)
            | E::ArrayStringHasOwn(a, b)
            | E::ArrayPush(a, b)
            | E::ArrayAt(a, b)
            | E::Index(a, b) => self.boxed(&mut [a, b])?,
            E::RecordDelete { object, absent, .. } => {
                if self.view(object.ty).is_some() {
                    return Err(self.locate(
                        unsupported(&format!(
                            "delete through {}, which a wider object type is assigned to",
                            describe(object.ty, self.types)
                        )),
                        value.span,
                    ));
                }
                self.boxed(&mut [object, absent])?;
            }
            E::ArrayFill(a, b, c, d) => self.boxed(&mut [a, b, c, d])?,
            E::Math(_, args)
            | E::NumberPredicate(_, args)
            | E::Intrinsic(_, args)
            | E::RegExp(_, args)
            | E::Call(_, args)
            | E::Host(_, args)
            | E::CheckedBigIntHost { args, .. } => {
                for arg in args {
                    self.expression(arg)?;
                }
            }
            E::Record(fields) | E::OptionalRecord(fields, _) => {
                for (_, field) in fields {
                    self.expression(field)?;
                }
            }
            E::Array(elements) => {
                for element in elements.iter_mut().flatten() {
                    self.expression(element)?;
                }
            }
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
            }
        }
        let span = value.span;
        let access = match &value.kind {
            E::Field(base, slot) => self
                .view(base.ty)
                .map(|shape| (shape, *slot, Access::Get(value.ty))),
            E::HasOwn(base, Some(slot)) => {
                self.view(base.ty).map(|shape| (shape, *slot, Access::Has))
            }
            _ => None,
        };
        if let Some((shape, slot, access)) = access {
            let index = self.accessor(shape, slot, access, span)?;
            let (E::Field(base, _) | E::HasOwn(base, _)) =
                std::mem::replace(&mut value.kind, E::Zero)
            else {
                unreachable!("matched above");
            };
            value.kind = E::Call(index, vec![*base]);
        }
        Ok(())
    }

    /// The accessor function of `access` to member `slot` of view `shape`.
    fn accessor(
        &mut self,
        shape: u32,
        slot: u32,
        access: Access,
        span: swc_common::Span,
    ) -> Result<usize> {
        if let Some(index) = self.accessors.get(&(shape, slot, access)) {
            return Ok(*index);
        }
        if self.next >= self.types.function_limit() {
            return Err(self.locate(invalid("too many functions across modules"), span));
        }
        let function = self
            .accessor_function(shape, slot, access, span)
            .map_err(|error| self.locate(error, span))?;
        let index = self.next;
        self.next += 1;
        self.created.push(function);
        self.accessors.insert((shape, slot, access), index);
        Ok(index)
    }
    fn accessor_function(
        &self,
        shape: u32,
        slot: u32,
        access: Access,
        span: swc_common::Span,
    ) -> Result<ir::Function> {
        let types = self.types;
        let view = Type::Record(shape);
        let wanted = types.records[shape as usize].fields[slot as usize].clone();
        let name = wanted.name.as_str();
        let (params, result) = match access {
            Access::Get(result) => (vec![view], result),
            Access::Set(value) => (vec![view, value], Type::Void),
            Access::Has => (vec![view], Type::Boolean),
            Access::Call(function) => {
                let Type::Function(id) = function else {
                    return Err(CompilerError::Internal(
                        "view call of a non-function".into(),
                    ));
                };
                let signature = &types.functions[id as usize];
                (
                    std::iter::once(view)
                        .chain(signature.params.iter().copied())
                        .collect(),
                    signature.result,
                )
            }
        };
        let mut locals = params.clone();
        let descriptor = locals.len() as u32;
        locals.push(Type::Number);
        let receiver = |ty: Type| local(span, 0, ty);
        let finish = |value: Expr| {
            if result == Type::Void {
                vec![S::Expr(value), S::Return(None)]
            } else {
                vec![S::Return(Some(value))]
            }
        };
        // The fallback call of a function field: hidden callee, argument and
        // output slots shared by every branch (one runs).
        let mut call_slots = None;
        if let Access::Call(function) = access {
            let callee = locals.len() as u32;
            locals.push(function);
            let arguments: Vec<u32> = params[1..]
                .iter()
                .map(|ty| {
                    locals.push(*ty);
                    locals.len() as u32 - 1
                })
                .collect();
            let output = (result != Type::Void).then(|| {
                locals.push(result);
                locals.len() as u32 - 1
            });
            call_slots = Some((callee, arguments, output));
        }
        let invoke = |callee: Expr| -> Expr {
            let (slot, arguments, output) = call_slots.clone().unwrap_or_default();
            Expr {
                span,
                ty: result,
                kind: E::FunctionCall {
                    callee: Box::new(callee),
                    ignored_receiver: None,
                    args: params[1..]
                        .iter()
                        .enumerate()
                        .map(|(i, ty)| local(span, i as u32 + 1, *ty))
                        .collect(),
                    slot,
                    arguments,
                    output,
                },
            }
        };
        let own = Expr {
            span,
            ty: wanted.ty,
            kind: E::Field(Box::new(receiver(view)), slot),
        };
        let mut body = Vec::new();
        for source in &types.subtyping.sources[&shape] {
            let shapes = match *source {
                Type::Record(id) => vec![id],
                Type::Class(class) => types.classes.family_shapes(class),
                _ => return Err(CompilerError::Internal("view source type".into())),
            };
            let branch = match access {
                Access::Get(result) => finish(self.read(*source, name, result, span)?),
                Access::Has => finish(self.has(*source, name, span)),
                Access::Set(value) => self.write(*source, shape, &wanted, value, span)?,
                Access::Call(function) => {
                    let Type::Function(id) = function else {
                        return Err(CompilerError::Internal("view call".into()));
                    };
                    match self.method(*source, name) {
                        Some((index, owner, returned)) => {
                            let call = Expr {
                                span,
                                ty: returned,
                                kind: E::Call(
                                    index,
                                    std::iter::once(receiver(Type::Class(owner)))
                                        .chain(
                                            types.functions[id as usize]
                                                .params
                                                .iter()
                                                .enumerate()
                                                .map(|(i, ty)| local(span, i as u32 + 1, *ty)),
                                        )
                                        .collect(),
                                ),
                            };
                            if result == Type::Void {
                                vec![S::Expr(call), S::Return(None)]
                            } else {
                                vec![S::Return(Some(convert(call, result, types)?))]
                            }
                        }
                        None => finish(invoke(self.read(*source, name, function, span)?)),
                    }
                }
            };
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsClass {
                        value: Box::new(receiver(view)),
                        shapes,
                        slot: descriptor,
                    },
                },
                branch,
                Vec::new(),
            ));
        }
        body.extend(match access {
            Access::Get(result) => finish(convert(own, result, types)?),
            Access::Has => finish(Expr {
                span,
                ty: Type::Boolean,
                kind: E::HasOwn(Box::new(receiver(view)), Some(slot)),
            }),
            Access::Set(_) => vec![
                S::SetField(receiver(view), slot, local(span, 1, params[1])),
                S::Return(None),
            ],
            Access::Call(_) => finish(invoke(own)),
        });
        Ok(ir::Function {
            shape: ir::ParameterShape::fixed(params.len()),
            self_slot: None,
            receiver: None,
            source: self
                .source
                .clone()
                .ok_or_else(|| CompilerError::Internal("view accessor source".into()))?,
            span,
            params,
            result,
            locals,
            body,
            cells: Default::default(),
            captures: Vec::new(),
            environment: false,
            generator: None,
        })
    }
    /// The read of member `name` of an object of `source` type, converted
    /// to `result`; a missing optional member reads `undefined`.
    fn read(&self, source: Type, name: &str, result: Type, span: swc_common::Span) -> Result<Expr> {
        let types = self.types;
        let receiver = |ty: Type| local(span, 0, ty);
        let value = match source {
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .position(|field| field.name == name)
                .map(|slot| Expr {
                    span,
                    ty: types.records[id as usize].fields[slot].ty,
                    kind: E::Field(Box::new(receiver(source)), slot as u32),
                }),
            Type::Class(class) => match types.classes.field(class, name) {
                Some((slot, field)) => Some(Expr {
                    span,
                    ty: field.ty,
                    kind: E::Field(Box::new(receiver(source)), slot),
                }),
                None => match types.classes.get(class).methods.get(name) {
                    Some(method) if method.getter => {
                        let (index, owner) = method.target();
                        Some(Expr {
                            span,
                            ty: method.signature.result,
                            kind: E::Call(index, vec![receiver(Type::Class(owner))]),
                        })
                    }
                    Some(_) => {
                        return Err(unsupported(&format!(
                            "method {name} of class {} as a function value through a structural \
                             view; call it instead",
                            types.classes.get(class).name
                        )));
                    }
                    None => None,
                },
            },
            _ => return Err(CompilerError::Internal("view source type".into())),
        };
        let value = value.unwrap_or(Expr {
            span,
            ty: Type::Undefined,
            kind: E::Integer(0),
        });
        convert(value, result, types)
    }
    /// The branches of the reserved record-to-JSON-object function: for each
    /// boxed closed record shape, a dictionary of its present fields (in
    /// canonical field order), each value as `unknown`.
    fn graph_branches(&self, graph: Type) -> Result<Vec<S>> {
        let types = self.types;
        let span = swc_common::DUMMY_SP;
        let Type::Dictionary(id) = graph else {
            return Err(CompilerError::Internal("record graph type".into()));
        };
        let dictionary = types.dictionaries[id as usize];
        let any = Type::Record(
            types
                .records
                .iter()
                .position(|r| r.fields.is_empty() && r.class.is_none())
                .ok_or_else(|| CompilerError::Internal("empty record shape".into()))?
                as u32,
        );
        // Locals: 0 the record, 1 the descriptor, 2 keys, 3 values.
        let (descriptor, keys, values) = (1, 2, 3);
        let push = |array: u32, ty: Type, item: Expr| {
            S::Expr(Expr {
                span,
                ty: types.number(),
                kind: E::ArrayPush(
                    Box::new(local(span, array, ty)),
                    Box::new(Expr {
                        span,
                        ty,
                        kind: E::Array(vec![Some(item)]),
                    }),
                ),
            })
        };
        let mut body = Vec::new();
        let boxed: Vec<u32> = types.subtyping.boxed.borrow().iter().copied().collect();
        for shape in boxed {
            let record = Type::Record(shape);
            let mut branch = vec![
                S::Set(
                    keys,
                    Expr {
                        span,
                        ty: dictionary.keys,
                        kind: E::Array(Vec::new()),
                    },
                ),
                S::Set(
                    values,
                    Expr {
                        span,
                        ty: dictionary.values,
                        kind: E::Array(Vec::new()),
                    },
                ),
            ];
            for (slot, field) in types.records[shape as usize].fields.iter().enumerate() {
                let value = unknown::widen(
                    Expr {
                        span,
                        ty: field.ty,
                        kind: E::Field(Box::new(local(span, 0, record)), slot as u32),
                    },
                    types,
                )?;
                let key = Expr {
                    span,
                    ty: Type::String,
                    kind: E::String(types.profile.string_bytes(&field.name)),
                };
                let entry = vec![
                    push(keys, dictionary.keys, key),
                    push(values, dictionary.values, value),
                ];
                if field.optional {
                    branch.push(S::If(
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::HasOwn(Box::new(local(span, 0, record)), Some(slot as u32)),
                        },
                        entry,
                        Vec::new(),
                    ));
                } else {
                    branch.extend(entry);
                }
            }
            branch.push(S::Return(Some(Expr {
                span,
                ty: graph,
                kind: E::Record(vec![
                    (dictionaries::KEYS, local(span, keys, dictionary.keys)),
                    (dictionaries::VALUES, local(span, values, dictionary.values)),
                ]),
            })));
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsClass {
                        value: Box::new(local(span, 0, any)),
                        shapes: vec![shape],
                        slot: descriptor,
                    },
                },
                branch,
                Vec::new(),
            ));
        }
        Ok(body)
    }
    /// The body of the reserved `"name" in value` function of view `shape`,
    /// which does not declare `name`: true for sources that have it.
    fn has_name(&self, shape: u32, name: &str) -> Vec<S> {
        let span = swc_common::DUMMY_SP;
        let view = Type::Record(shape);
        let mut body = Vec::new();
        for source in &self.types.subtyping.sources[&shape] {
            let shapes = match *source {
                Type::Record(id) => vec![id],
                Type::Class(class) => self.types.classes.family_shapes(class),
                _ => continue,
            };
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsClass {
                        value: Box::new(local(span, 0, view)),
                        shapes,
                        slot: 1,
                    },
                },
                vec![S::Return(Some(self.has(*source, name, span)))],
                Vec::new(),
            ));
        }
        body.push(S::Return(Some(constant(span, false))));
        body
    }
    fn has(&self, source: Type, name: &str, span: swc_common::Span) -> Expr {
        let types = self.types;
        match source {
            Type::Record(id) => {
                match types.records[id as usize]
                    .fields
                    .iter()
                    .position(|field| field.name == name)
                {
                    Some(slot) => Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::HasOwn(Box::new(local(span, 0, source)), Some(slot as u32)),
                    },
                    None => constant(span, false),
                }
            }
            Type::Class(class) => constant(
                span,
                types.classes.field(class, name).is_some()
                    || types.classes.get(class).methods.contains_key(name),
            ),
            _ => constant(span, false),
        }
    }
    fn write(
        &self,
        source: Type,
        shape: u32,
        wanted: &ir::RecordField,
        value: Type,
        span: swc_common::Span,
    ) -> Result<Vec<S>> {
        let types = self.types;
        let name = &wanted.name;
        let target = match source {
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .position(|field| field.name == *name)
                .map(|slot| (slot as u32, types.records[id as usize].fields[slot].ty)),
            Type::Class(class) => match types.classes.field(class, name) {
                Some((_, field)) if field.readonly => {
                    return Err(type_error(format!(
                        "cannot assign {name} through {}: class {} assigned to it declares \
                         {name} readonly",
                        describe(Type::Record(shape), types),
                        describe(source, types)
                    )));
                }
                Some((slot, field)) => Some((slot, field.ty)),
                None => None,
            },
            _ => None,
        };
        match target {
            Some((slot, ty)) if ty == value => Ok(vec![
                S::SetField(local(span, 0, source), slot, local(span, 1, value)),
                S::Return(None),
            ]),
            Some((_, ty)) => Err(type_error(format!(
                "cannot assign {name} through {}: an object of type {} assigned to it declares \
                 {name} as {}, not {}",
                describe(Type::Record(shape), types),
                describe(source, types),
                describe(ty, types),
                describe(value, types)
            ))),
            None => Err(type_error(format!(
                "cannot assign {name} through {}: an object of type {} assigned to it has no \
                 data field {name}",
                describe(Type::Record(shape), types),
                describe(source, types)
            ))),
        }
    }
    /// The direct method call target of member `name` of a class source:
    /// (function, receiver class, result type).
    fn method(&self, source: Type, name: &str) -> Option<(usize, u32, Type)> {
        let Type::Class(class) = source else {
            return None;
        };
        if self.types.classes.field(class, name).is_some() {
            return None;
        }
        let method = self.types.classes.get(class).methods.get(name)?;
        if method.getter {
            return None;
        }
        let (index, owner) = method.target();
        Some((index, owner, method.signature.result))
    }
}

/// `value` as type `target`: a view of a record (retyped) or the nominal
/// widening (union boxing, literal erasure of the context).
fn convert(value: Expr, target: Type, types: &ir::ValueTypes) -> Result<Expr> {
    if value.ty == target {
        return Ok(value);
    }
    if matches!(target, Type::Record(_)) && matches!(value.ty, Type::Record(_) | Type::Class(_)) {
        return Ok(viewed(value, target));
    }
    let value = unions::widen(value, Some(target), types)?;
    if value.ty != target {
        return Err(CompilerError::Internal(format!(
            "structural view conversion from {} to {target}",
            value.ty
        )));
    }
    Ok(value)
}

impl Lower<'_> {
    /// A closed record held by the `unknown` cell `cell` read as a JSON
    /// object `Record<string, unknown>` (v38): the reserved function that
    /// `resolve` completes for every record shape boxed into `unknown`.
    pub(super) fn record_graph(&mut self, cell: Expr, span: swc_common::Span) -> Result<Expr> {
        let graph = dictionaries::intern(Type::Unknown, self.types)?;
        let any = records::intern(Vec::new(), self.types)?;
        let index = match self.types.subtyping.graph {
            Some((index, _)) => index,
            None => {
                let index = self.reserve_function("too many functions across modules")?;
                self.types.subtyping.graph = Some((index, graph));
                let Type::Dictionary(id) = graph else {
                    return Err(CompilerError::Internal("record graph type".into()));
                };
                let dictionary = self.types.dictionaries[id as usize];
                let error = self.array_error(
                    varta_common::value::ErrorClass::TypeError,
                    "a class instance held by unknown cannot be read as a JSON object",
                    span,
                )?;
                self.generated.insert(
                    index,
                    ir::Function {
                        shape: ir::ParameterShape::fixed(1),
                        self_slot: None,
                        receiver: None,
                        source: self.source.clone(),
                        span,
                        params: vec![any],
                        result: graph,
                        locals: vec![any, Type::Number, dictionary.keys, dictionary.values],
                        body: vec![S::Throw(error)],
                        cells: Default::default(),
                        captures: Vec::new(),
                        environment: false,
                        generator: None,
                    },
                );
                index
            }
        };
        Ok(Expr {
            span,
            ty: graph,
            kind: E::Call(
                index,
                vec![Expr {
                    span,
                    ty: any,
                    kind: E::UnboxUnion(Box::new(cell)),
                }],
            ),
        })
    }
    /// `"name" in value` for a record or class instance value (v38): a
    /// record's own (present) field, an inherited `Object.prototype` name or
    /// an instance's field, method or getter. A field the static type does
    /// not declare is absent; a view resolves declared fields per layout.
    pub(super) fn object_in(
        &mut self,
        name: &str,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let sequence = |value: Expr, answer: bool| Expr {
            span,
            ty: Type::Boolean,
            kind: E::Sequence(Box::new(value), Box::new(constant(span, answer))),
        };
        match value.ty {
            Type::Record(_) if records::inherited(name) => Ok(sequence(value, true)),
            Type::Record(id) => {
                let slot = self.types.records[id as usize]
                    .fields
                    .iter()
                    .position(|field| field.name == name)
                    .map(|slot| slot as u32);
                if slot.is_none() {
                    return self.undeclared_in(id, name, value, span);
                }
                Ok(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::HasOwn(Box::new(value), slot),
                })
            }
            Type::Class(class) => {
                let declared = self.types.classes.field(class, name).is_some()
                    || self.types.classes.get(class).methods.contains_key(name)
                    || records::inherited(name);
                Ok(sequence(value, declared))
            }
            _ => Ok(sequence(value, false)),
        }
    }
    /// `"name" in value` for a name record `shape` does not declare (v38):
    /// a view may hold an object that has it, so the test calls the reserved
    /// per-(shape, name) function that `resolve` completes.
    pub(super) fn undeclared_in(
        &mut self,
        shape: u32,
        name: &str,
        value: Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let key = (shape, name.to_owned());
        let index = match self.types.subtyping.names.get(&key) {
            Some(index) => *index,
            None => {
                let index = self.reserve_function("too many functions across modules")?;
                let view = Type::Record(shape);
                self.generated.insert(
                    index,
                    ir::Function {
                        shape: ir::ParameterShape::fixed(1),
                        self_slot: None,
                        receiver: None,
                        source: self.source.clone(),
                        span,
                        params: vec![view],
                        result: Type::Boolean,
                        locals: vec![view, Type::Number],
                        body: vec![S::Return(Some(constant(span, false)))],
                        cells: Default::default(),
                        captures: Vec::new(),
                        environment: false,
                        generator: None,
                    },
                );
                self.types.subtyping.names.insert(key, index);
                index
            }
        };
        Ok(Expr {
            span,
            ty: Type::Boolean,
            kind: E::Call(index, vec![value]),
        })
    }
}

/// Operands of an identity comparison of two object types (v38): one object
/// may be viewed through several record shapes, so two record or instance
/// values of different types may be the same object. The right operand is
/// retyped to the left's type and the handles are compared, instead of
/// folding the comparison to `false` by type.
pub(super) fn identity_operands(
    op: ast::BinaryOp,
    left: Expr,
    right: Expr,
    types: &ir::ValueTypes,
) -> (Expr, Expr) {
    use ast::BinaryOp::*;
    let object = |ty: Type| matches!(ty, Type::Record(_) | Type::Class(_));
    if types.profile.structural_subtyping()
        && matches!(op, EqEq | EqEqEq | NotEq | NotEqEq)
        && left.ty != right.ty
        && object(left.ty)
        && object(right.ty)
    {
        let ty = left.ty;
        (
            left,
            Expr {
                span: right.span,
                ty,
                kind: E::Reinterpret(Box::new(right)),
            },
        )
    } else {
        (left, right)
    }
}
