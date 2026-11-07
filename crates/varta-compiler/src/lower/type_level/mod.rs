//! The K9 type level (v38 `type_level`): conditional types with `infer`,
//! mapped types, template literal and intrinsic string types, the built-in
//! utility types, `keyof`/indexed access/intersections over every closed
//! type, `typeof` queries, call and construct signatures, variadic and rest
//! tuples, type predicates, assertion functions, `satisfies` and polymorphic
//! `this`.
//!
//! Varta monomorphizes: every type parameter is bound to a resolved type
//! before a type expression is lowered, so each operator here evaluates over
//! resolved [`Type`]s and yields an existing closed type (records, literal
//! sets, tuples, unions, function types). Nothing generic or deferred reaches
//! emission. A construct without a sound static layout is refused with a
//! diagnostic that names it.
use super::*;
use std::collections::BTreeSet;

mod conditional;
mod constructors;
mod containers;
mod covariance;
mod literal_inference;
mod mapped;
mod objects;
mod overloads;
mod patterns;
mod predicates;
mod queries;
mod relation;
mod satisfies;
mod template_types;
mod tuple_types;
mod utilities;

pub(super) use containers::covariant_array;
pub(super) use covariance::resolve as resolve_covariance;
pub(crate) use patterns::Pattern;
pub(super) use patterns::{describe as describe_pattern, value_in};

/// Whether `expected` is a string literal type or a union with one (a
/// template literal expression there is typed by its parts, v38).
pub(super) fn literal_context(expected: Option<Type>, types: &ir::ValueTypes) -> bool {
    types.profile.type_level()
        && expected.is_some_and(|expected| {
            unions::members(expected, types)
                .iter()
                .any(|member| matches!(member, Type::Literal(ir::LiteralBase::String, _)))
        })
}

/// A string literal set without the strings its pattern members match
/// (TypeScript reduces such a union to its template literal types).
pub(super) fn reduce_patterns(
    values: BTreeSet<literals::LiteralValue>,
    types: &ir::ValueTypes,
) -> BTreeSet<literals::LiteralValue> {
    let patterns: Vec<&Pattern> = values
        .iter()
        .filter_map(|value| match value {
            literals::LiteralValue::Pattern(id) => Some(types.literals.pattern(*id)),
            _ => None,
        })
        .collect();
    if patterns.is_empty() {
        return values;
    }
    values
        .iter()
        .filter(|value| {
            !matches!(value, literals::LiteralValue::String(_))
                || !literals::string_units(value, types.profile).is_some_and(|units| {
                    patterns
                        .iter()
                        .any(|pattern| patterns::text_matches(&units, pattern))
                })
        })
        .cloned()
        .collect()
}
pub(super) use relation::extends;

/// Nesting bound of type-level evaluation (conditional branches, mapped
/// members and recursive generic aliases); TypeScript's own instantiation
/// depth limit is of the same order.
pub(crate) const MAX_TYPE_DEPTH: usize = 64;

/// K9 type-level state of one compilation.
#[derive(Default)]
pub(crate) struct TypeLevel {
    /// Type predicates and assertion signatures, by function index.
    predicates: BTreeMap<usize, predicates::Predicate>,
    /// Generic call sites by (module, span): the instance function index,
    /// so a predicate of a generic function narrows at its call.
    calls: BTreeMap<(usize, u32, u32), usize>,
    /// The class whose member signature is being lowered (`this` types).
    this_class: Option<u32>,
    /// Methods declared to return the polymorphic `this` type.
    this_methods: BTreeSet<(u32, String)>,
    /// Module-scope value declarations for `typeof` queries.
    values: queries::Values,
    /// Generic alias instantiations being resolved (recursive aliases).
    resolving: Vec<(usize, String, Vec<Type>)>,
    /// Bindings (declaring function, slot) a type predicate narrowed: their
    /// narrowed reads are tested at run time.
    checked: BTreeSet<(usize, u32)>,
    /// Bindings (declaring function, slot) of a `const` declared by a
    /// literal: their literal type widens in a mutable location.
    widening: BTreeSet<(usize, u32)>,
    /// The visible local bindings' types while a statement that names one
    /// in a `typeof` query is lowered (by name; inner scopes shadow).
    locals: Option<BTreeMap<String, Type>>,
    /// Methods with a `this is T` predicate, by (declaring class, name).
    this_predicates: BTreeMap<(u32, String), Type>,
    /// Overload sets of overloaded function types: each overload's function
    /// type, in declaration order.
    overloads: Vec<Vec<Type>>,
    /// Class-instance arrays viewed under a wider element type: (source
    /// array type, target array type), resolved by `covariance::resolve`.
    covariance: std::cell::RefCell<BTreeSet<(u32, u32)>>,
    /// Current evaluation depth.
    depth: usize,
}

/// Evaluate a K9 type expression; `None` leaves it to the earlier rules.
pub(super) fn evaluate(ty: &ast::TsType, types: &mut ir::ValueTypes) -> Result<Option<Type>> {
    if !types.profile.type_level() {
        return Ok(None);
    }
    Ok(Some(match ty {
        ast::TsType::TsConditionalType(conditional) => {
            nested(types, |t| conditional::evaluate(conditional, t))?
        }
        ast::TsType::TsMappedType(mapped) => nested(types, |t| mapped::evaluate(mapped, t))?,
        ast::TsType::TsLitType(ast::TsLitType {
            lit: ast::TsLit::Tpl(template),
            ..
        }) => template_types::evaluate(template, types)?,
        ast::TsType::TsTypeQuery(query) => queries::type_of(query, types)?,
        ast::TsType::TsTypeOperator(operator) => match operator.op {
            ast::TsTypeOperatorOp::KeyOf => objects::key_of(&operator.type_ann, types)?,
            ast::TsTypeOperatorOp::Unique => {
                return Err(unsupported(
                    "unique symbol type (a unique symbol needs Symbol values, which this profile does not admit)",
                ));
            }
            ast::TsTypeOperatorOp::ReadOnly => match &*operator.type_ann {
                ast::TsType::TsTupleType(tuple) => tuple_types::annotation(tuple, true, types)?,
                // `readonly T[]` keeps its earlier rule; readonly erases.
                ast::TsType::TsArrayType(_) => return Ok(None),
                other => {
                    return Err(unsupported(&format!(
                        "readonly operator on {}",
                        kind_name(other)
                    )));
                }
            },
        },
        ast::TsType::TsIndexedAccessType(access) => objects::indexed(access, types)?,
        ast::TsType::TsUnionOrIntersectionType(
            ast::TsUnionOrIntersectionType::TsIntersectionType(intersection),
        ) => objects::intersection(intersection, types)?,
        ast::TsType::TsTupleType(tuple) => tuple_types::annotation(tuple, false, types)?,
        ast::TsType::TsInferType(infer) => {
            return Err(type_error(format!(
                "infer {} is only permitted in the extends clause of a conditional type",
                infer.type_param.name.sym
            )));
        }
        ast::TsType::TsThisType(_) => this_type(types)?,
        ast::TsType::TsTypePredicate(predicate) => predicates::result(predicate, types)?,
        ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsConstructorType(
            constructor,
        )) => constructors::constructor_type(constructor, types)?,
        // `<T>(value: T) => T`: the erased signature of a generic function
        // value, instantiated at each call.
        ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsFnType(function))
            if function.type_params.is_some() =>
        {
            constructors::generic_signature(
                function.span,
                &function.params,
                Some(&function.type_ann),
                function.type_params.as_deref(),
                types,
            )?
        }
        ast::TsType::TsTypeLit(literal) if constructors::callable(literal) => {
            constructors::type_literal(literal, types)?
        }
        ast::TsType::TsTypeRef(reference) => match utilities::reference(reference, types)? {
            Some(ty) => ty,
            None => return Ok(None),
        },
        ast::TsType::TsImportType(_) => {
            return Err(unsupported(
                "import type (import the type with an import declaration instead)",
            ));
        }
        _ => return Ok(None),
    }))
}

/// The type of `expression as const` (readonly tuples and records of
/// literal types); `None` when it is not statically inferable here.
pub(super) fn const_type(expression: &ast::Expr, types: &mut ir::ValueTypes) -> Option<Type> {
    if !types.profile.type_level() {
        return None;
    }
    queries::const_type(expression, types)
}

/// The ECMAScript string of a number or boolean literal value (a template
/// literal's conversion); `None` for a string value.
pub(super) fn literal_text(value: &literals::LiteralValue) -> Option<String> {
    match value {
        literals::LiteralValue::Number(bits) => {
            Some(template_types::number_text(f64::from_bits(*bits)))
        }
        literals::LiteralValue::Boolean(flag) => Some(flag.to_string()),
        literals::LiteralValue::String(_) | literals::LiteralValue::Pattern(_) => None,
    }
}

/// Whether a binding pattern's annotation contains a `typeof` query (its
/// type is resolved when the declaration is lowered, v38).
pub(super) fn queries_type(pattern: &ast::Pat, types: &ir::ValueTypes) -> bool {
    use swc_ecma_visit::{Visit, VisitWith};
    struct Query(bool);
    impl Visit for Query {
        fn visit_ts_type_query(&mut self, _: &ast::TsTypeQuery) {
            self.0 = true;
        }
    }
    let ast::Pat::Ident(binding) = pattern else {
        return false;
    };
    let Some(annotation) = &binding.type_ann else {
        return false;
    };
    if !types.profile.type_level() {
        return false;
    }
    let mut query = Query(false);
    annotation.visit_with(&mut query);
    query.0
}

/// Run one nested evaluation step under the depth bound.
fn nested<R>(
    types: &mut ir::ValueTypes,
    evaluate: impl FnOnce(&mut ir::ValueTypes) -> Result<R>,
) -> Result<R> {
    if types.type_level.depth >= MAX_TYPE_DEPTH {
        return Err(invalid(format!(
            "type instantiation is excessively deep (more than {MAX_TYPE_DEPTH} nested evaluations)"
        )));
    }
    types.type_level.depth += 1;
    let result = evaluate(types);
    types.type_level.depth -= 1;
    result
}

/// Evaluate `ty` with `bindings` added to the visible type parameters.
fn with_bindings(
    ty: &ast::TsType,
    bindings: &generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    if bindings.is_empty() {
        return keyword_type(ty, types);
    }
    let mut frame = generics::visible(types);
    frame.extend(bindings.iter().map(|(name, ty)| (name.clone(), *ty)));
    generics::with_frame(types, frame, |t| keyword_type(ty, t))
}

/// The name of a type parameter `ty` refers to directly (a naked type
/// parameter: conditional and homomorphic mapped types distribute over it).
fn naked_parameter(ty: &ast::TsType, types: &ir::ValueTypes) -> Option<String> {
    let ast::TsType::TsTypeRef(reference) = ty else {
        return None;
    };
    let ast::TsEntityName::Ident(id) = &reference.type_name else {
        return None;
    };
    (reference.type_params.is_none() && generics::parameter(ty, types).is_some())
        .then(|| id.sym.to_string())
}

/// A union of evaluated types: `never` for none, the member for one.
fn union_of(members: Vec<Type>, types: &mut ir::ValueTypes) -> Result<Type> {
    let members: Vec<Type> = members
        .into_iter()
        .filter(|ty| *ty != Type::Never)
        .collect();
    match members.as_slice() {
        [] => Ok(Type::Never),
        [one] => Ok(*one),
        _ => unions::intern(members, types),
    }
}

/// The members of `ty` as TypeScript distributes them: union members, each
/// value of a literal set and `true`/`false` of `boolean`.
fn split(ty: Type, types: &mut ir::ValueTypes) -> Vec<Type> {
    let mut members = Vec::new();
    for member in unions::members(ty, types) {
        match member {
            Type::Literal(base, id) => {
                let values: Vec<_> = types.literals.values(id).iter().cloned().collect();
                if values.len() == 1 {
                    members.push(member);
                    continue;
                }
                for value in values {
                    match literals::single(base, value, types) {
                        Ok(single) => members.push(single),
                        Err(_) => members.push(member),
                    }
                }
            }
            Type::Boolean => {
                for value in [false, true] {
                    members.push(literals::boolean(value, types).unwrap_or(Type::Boolean));
                }
            }
            other => members.push(other),
        }
    }
    members.dedup();
    members
}

/// `this` in a class member signature: the class (polymorphic `this` is
/// specialized to the receiver's class at each call).
fn this_type(types: &mut ir::ValueTypes) -> Result<Type> {
    if let Some(class) = types.type_level.this_class {
        return Ok(Type::Class(class));
    }
    // In an interface or object type alias, `this` is the declared type
    // itself (a recursive reference resolved through its reservation).
    match types.type_level.resolving.last() {
        Some((_, name, arguments)) if arguments.is_empty() => {
            let name = name.clone();
            named::resolve(&name, types)
        }
        Some((_, name, _)) => Err(unsupported(&format!(
            "this type in the generic declaration {name} (each instantiation would refer to itself)"
        ))),
        None => Err(type_error(
            "a this type is only available in a class, interface or object type member",
        )),
    }
}

/// Lower a class member signature with `this` bound to `class`; a method
/// whose result is the polymorphic `this` type is recorded under `name`.
pub(super) fn member_signature(
    types: &mut ir::ValueTypes,
    (class, name, function): (u32, &str, &ast::Function),
    lower: impl FnOnce(&mut ir::ValueTypes) -> Result<Signature>,
) -> Result<Signature> {
    if !types.profile.type_level() {
        return lower(types);
    }
    let outer = types.type_level.this_class.replace(class);
    let result = lower(types).and_then(|signature| {
        predicates::note_method((class, name), function, types).map(|()| signature)
    });
    types.type_level.this_class = outer;
    if let Some(annotation) = &function.return_type
        && matches!(&*annotation.type_ann, ast::TsType::TsThisType(_))
    {
        if function.body.as_ref().is_some_and(returns_other) {
            return Err(type_error(format!(
                "method {name} returns the polymorphic this type, so every return must return this (another instance has no subclass identity)"
            )));
        }
        types
            .type_level
            .this_methods
            .insert((class, name.to_owned()));
    }
    result
}

/// Whether a method body returns anything but `this` (nested functions
/// have their own returns).
fn returns_other(body: &ast::FunctionBody) -> bool {
    use swc_ecma_visit::{Visit, VisitWith};
    struct Returns(bool);
    impl Visit for Returns {
        fn visit_return_stmt(&mut self, statement: &ast::ReturnStmt) {
            self.0 |= !statement
                .arg
                .as_deref()
                .is_some_and(|arg| matches!(stmt::unparen(arg), ast::Expr::This(_)));
        }
        fn visit_function(&mut self, _: &ast::Function) {}
        fn visit_arrow_expr(&mut self, _: &ast::ArrowExpr) {}
        fn visit_class(&mut self, _: &ast::Class) {}
    }
    let mut visitor = Returns(false);
    body.visit_with(&mut visitor);
    visitor.0
}

/// Whether method `name` declared by class `owner` returns `this`.
pub(super) fn returns_this(types: &ir::ValueTypes, owner: u32, name: &str) -> bool {
    types
        .type_level
        .this_methods
        .contains(&(owner, name.to_owned()))
}

impl Lower<'_> {
    /// The result of a call of method `name` declared by `owner` on a
    /// receiver of class `class`: a polymorphic `this` result has the
    /// receiver's class (the value is the receiver itself or another
    /// instance of its class family, so the record layout is shared).
    pub(in crate::lower) fn polymorphic_this(
        &self,
        result: Expr,
        class: u32,
        owner: u32,
        name: &str,
    ) -> Expr {
        if class == owner
            || result.ty != Type::Class(owner)
            || !returns_this(self.types, owner, name)
        {
            return result;
        }
        Expr {
            span: result.span,
            ty: Type::Class(class),
            kind: E::Reinterpret(Box::new(result)),
        }
    }
}

/// Record the predicate of a top-level or generic-instance function signature.
pub(super) fn note_signature(
    declaration: &ast::FnDecl,
    signature: &Signature,
    types: &mut ir::ValueTypes,
) -> Result<()> {
    predicates::note(declaration, signature, types)
}

/// Record the function index a generic call instantiated (predicate guards).
pub(super) fn note_call(types: &mut ir::ValueTypes, span: swc_common::Span, index: usize) {
    if types.profile.type_level() {
        let module = named::current(types);
        types
            .type_level
            .calls
            .insert((module, span.lo.0, span.hi.0), index);
    }
}

/// Register the module-scope values of `module` for `typeof` queries.
pub(super) fn register_values(
    module: usize,
    unit: &crate::modules::Unit,
    graph: &crate::modules::Graph,
    types: &mut ir::ValueTypes,
) {
    if types.profile.type_level() {
        queries::register(module, unit, graph, types);
    }
}

/// Enter a recursive generic alias instantiation (`named::instantiate`):
/// `Ok(true)` when the same declaration with other type arguments is being
/// resolved and the new instantiation may proceed.
pub(super) fn enter_recursive(
    key: &(usize, String),
    arguments: &[Type],
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    if !types.profile.type_level() || arguments.is_empty() {
        return Ok(false);
    }
    let resolving = &types.type_level.resolving;
    if resolving
        .iter()
        .any(|(m, n, a)| (*m, n) == (key.0, &key.1) && a == arguments)
    {
        return Ok(false);
    }
    if resolving.len() >= MAX_TYPE_DEPTH {
        return Err(invalid(format!(
            "type instantiation of {} is excessively deep (more than {MAX_TYPE_DEPTH} nested instantiations)",
            key.1
        )));
    }
    Ok(true)
}
pub(super) fn push_instantiation(
    key: &(usize, String),
    arguments: &[Type],
    types: &mut ir::ValueTypes,
) {
    if types.profile.type_level() {
        types
            .type_level
            .resolving
            .push((key.0, key.1.clone(), arguments.to_vec()));
    }
}
pub(super) fn pop_instantiation(types: &mut ir::ValueTypes) {
    if types.profile.type_level() {
        types.type_level.resolving.pop();
    }
}

/// A short name of a type expression kind for diagnostics.
fn kind_name(ty: &ast::TsType) -> &'static str {
    match ty {
        ast::TsType::TsKeywordType(_) => "a keyword type",
        ast::TsType::TsTypeRef(_) => "a type reference",
        ast::TsType::TsTypeLit(_) => "an object type",
        ast::TsType::TsUnionOrIntersectionType(_) => "a union or intersection type",
        ast::TsType::TsFnOrConstructorType(_) => "a function type",
        ast::TsType::TsLitType(_) => "a literal type",
        _ => "this type",
    }
}
