//! Conditional types `C extends E ? T : F` (TypeScript Handbook, Conditional
//! Types). The check type is resolved, so the condition is decided at once:
//! a naked type parameter bound to a union distributes over its members
//! (`never` yields `never`), `infer X` positions of the extends pattern bind
//! the matched part of the check type (with an optional `extends`
//! constraint) and only the selected branch is evaluated.
use super::*;

pub(super) fn evaluate(
    conditional: &ast::TsConditionalType,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let check = keyword_type(&conditional.check_type, types)?;
    if let Some(name) = naked_parameter(&conditional.check_type, types) {
        if check == Type::Never {
            return Ok(Type::Never);
        }
        let members = split(check, types);
        if members.len() > 1 {
            let mut results = Vec::new();
            for member in members {
                let mut frame = generics::visible(types);
                frame.insert(name.clone(), member);
                results.push(generics::with_frame(types, frame, |t| {
                    branch(conditional, member, t)
                })?);
            }
            return union_of(results, types);
        }
    }
    branch(conditional, check, types)
}

fn branch(
    conditional: &ast::TsConditionalType,
    check: Type,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let mut bindings = generics::Frame::new();
    if matches(&conditional.extends_type, check, &mut bindings, types)? {
        with_bindings(&conditional.true_type, &bindings, types)
    } else {
        keyword_type(&conditional.false_type, types)
    }
}

/// Whether a type expression declares an `infer` position.
pub(super) fn has_infer(ty: &ast::TsType) -> bool {
    use swc_ecma_visit::{Visit, VisitWith};
    struct Infer(bool);
    impl Visit for Infer {
        fn visit_ts_infer_type(&mut self, _: &ast::TsInferType) {
            self.0 = true;
        }
        // An inner conditional type owns the infer positions of its own
        // extends clause.
        fn visit_ts_conditional_type(&mut self, conditional: &ast::TsConditionalType) {
            conditional.check_type.visit_with(self);
        }
    }
    let mut visitor = Infer(false);
    ty.visit_with(&mut visitor);
    visitor.0
}

/// Match `ty` against the extends pattern, binding its `infer` positions.
pub(super) fn matches(
    pattern: &ast::TsType,
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    if let ast::TsType::TsLitType(ast::TsLitType {
        lit: ast::TsLit::Tpl(template),
        ..
    }) = pattern
    {
        return template_types::matches(template, ty, bindings, types);
    }
    if !has_infer(pattern) {
        let target = with_bindings(pattern, bindings, types)?;
        return Ok(extends(ty, target, types));
    }
    match pattern {
        ast::TsType::TsParenthesizedType(inner) => matches(&inner.type_ann, ty, bindings, types),
        ast::TsType::TsInferType(infer) => bind(infer, ty, bindings, types),
        ast::TsType::TsArrayType(array) => element_of(&array.elem_type, ty, bindings, types),
        ast::TsType::TsTypeOperator(operator) if operator.op == ast::TsTypeOperatorOp::ReadOnly => {
            matches(&operator.type_ann, ty, bindings, types)
        }
        ast::TsType::TsTupleType(tuple) => match ty {
            Type::Tuple(id) => tuple_pattern(tuple, id, bindings, types),
            _ => Ok(false),
        },
        ast::TsType::TsTypeRef(reference) => reference_pattern(reference, ty, bindings, types),
        ast::TsType::TsFnOrConstructorType(function) => {
            function_pattern(function, ty, bindings, types)
        }
        ast::TsType::TsTypeLit(literal) => object_pattern(literal, ty, bindings, types),
        ast::TsType::TsLitType(ast::TsLitType {
            lit: ast::TsLit::Tpl(template),
            ..
        }) => template_types::matches(template, ty, bindings, types),
        ast::TsType::TsUnionOrIntersectionType(ast::TsUnionOrIntersectionType::TsUnionType(
            union,
        )) => union_pattern(union, ty, bindings, types),
        // `A & B`: the check type is assignable to the intersection when it
        // is assignable to every member, so each member matches it (sharing
        // the bindings; a name inferred twice joins its candidates).
        ast::TsType::TsUnionOrIntersectionType(
            ast::TsUnionOrIntersectionType::TsIntersectionType(intersection),
        ) => {
            for member in &intersection.types {
                if !matches(member, ty, bindings, types)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        _ => Err(unsupported("infer in this position of an extends pattern")),
    }
}

/// `infer X` / `infer X extends C`: bind X, or join a second candidate of a
/// covariant position into a union (TypeScript's candidate union).
fn bind(
    infer: &ast::TsInferType,
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let name = infer.type_param.name.sym.to_string();
    if let Some(constraint) = &infer.type_param.constraint {
        let bound = with_bindings(constraint, bindings, types)?;
        if !extends(ty, bound, types) {
            return Ok(false);
        }
    }
    let ty = match bindings.get(&name) {
        Some(previous) if *previous != ty => union_of(vec![*previous, ty], types)?,
        _ => ty,
    };
    bindings.insert(name, ty);
    Ok(true)
}

/// `E[]` against an array (its element) or a tuple (its element union).
fn element_of(
    element: &ast::TsType,
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let element_type = match ty {
        Type::Array(id) => types.arrays[id as usize],
        Type::Tuple(id) => {
            let tuple = types.tuples[id as usize].clone();
            let members: Vec<Type> = tuple
                .elements
                .iter()
                .chain(tuple.rest.iter())
                .copied()
                .collect();
            union_of(members, types)?
        }
        _ => return Ok(false),
    };
    matches(element, element_type, bindings, types)
}

/// `[infer H, ...infer R]` and other tuple patterns with at most one rest.
fn tuple_pattern(
    pattern: &ast::TsTupleType,
    id: u32,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let source = types.tuples[id as usize].clone();
    let rest_at = pattern.elem_types.iter().position(|element| {
        matches!(&*element.ty, ast::TsType::TsRestType(_))
            || matches!(&element.label, Some(ast::Pat::Rest(_)))
    });
    let Some(rest_at) = rest_at else {
        if source.rest.is_some() {
            return Ok(false);
        }
        if pattern.elem_types.len() != source.elements.len() {
            return Ok(false);
        }
        for (index, (element, ty)) in pattern
            .elem_types
            .iter()
            .zip(source.elements.iter())
            .enumerate()
        {
            let optional_source = index >= source.required;
            // `(infer X)?` / `x?: infer X` against an optional element binds
            // its declared type (without the `undefined` of absence).
            let (element_pattern, optional_pattern) = match &*element.ty {
                ast::TsType::TsOptionalType(optional) => (&*optional.type_ann, true),
                other => (
                    other,
                    matches!(&element.label, Some(ast::Pat::Ident(id)) if id.id.optional),
                ),
            };
            if optional_source && !optional_pattern {
                return Ok(false);
            }
            let ty = if optional_source {
                tuple_types::strip_undefined(*ty, types)?
            } else {
                *ty
            };
            if !matches(element_pattern, ty, bindings, types)? {
                return Ok(false);
            }
        }
        return Ok(true);
    };

    let suffix = pattern.elem_types.len() - rest_at - 1;
    if !source.trailing.is_empty() {
        return trailing_pattern(pattern, &source, (rest_at, suffix), bindings, types);
    }
    if source.required < source.elements.len() || source.rest.is_some() && suffix > 0 {
        return Err(unsupported(
            "infer over a tuple with optional elements, or a rest-element suffix pattern over an open tuple",
        ));
    }
    if source.elements.len() < rest_at + suffix {
        return Ok(false);
    }
    for (element, ty) in pattern.elem_types[..rest_at].iter().zip(&source.elements) {
        if !matches(&element.ty, *ty, bindings, types)? {
            return Ok(false);
        }
    }
    let middle_end = source.elements.len() - suffix;
    for (element, ty) in pattern.elem_types[rest_at + 1..]
        .iter()
        .zip(&source.elements[middle_end..])
    {
        if !matches(&element.ty, *ty, bindings, types)? {
            return Ok(false);
        }
    }
    let middle = source.elements[rest_at..middle_end].to_vec();
    let count = middle.len();
    let rest_tuple = tuple_types::tuple_of(middle, count, source.rest, types)?;
    let rest_pattern = match &*pattern.elem_types[rest_at].ty {
        ast::TsType::TsRestType(rest) => &*rest.type_ann,
        other => other,
    };
    matches(rest_pattern, rest_tuple, bindings, types)
}

/// `Array<infer E>`, `ReadonlyArray<infer E>`, `Promise<infer V>` and
/// `PromiseLike<infer V>`; other generic references are not invertible.
fn reference_pattern(
    reference: &ast::TsTypeRef,
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let ast::TsEntityName::Ident(id) = &reference.type_name else {
        return Err(unsupported("infer inside a qualified type reference"));
    };
    let name = id.sym.as_ref();
    let params = reference
        .type_params
        .as_ref()
        .map_or(&[][..], |p| &p.params[..]);
    if named::declared(types, name) {
        return declared_pattern(name, params, ty, bindings, types);
    }
    match (name, params) {
        ("Array" | "ReadonlyArray", [element]) => element_of(element, ty, bindings, types),
        ("Promise" | "PromiseLike", [value]) => match ty {
            Type::Promise(id) => {
                let value_type = types.promises[id as usize].value;
                matches(value, value_type, bindings, types)
            }
            _ => Ok(false),
        },
        _ => Err(unsupported(&format!(
            "infer inside the type arguments of {name}"
        ))),
    }
}

/// `(...args: infer P) => infer R`, `(a: infer A) => void` and
/// `new (...args: infer P) => infer I` against a function type.
fn function_pattern(
    function: &ast::TsFnOrConstructorType,
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let Type::Function(id) = ty else {
        return Ok(false);
    };
    let signature = types.functions[id as usize].clone();
    let (params, result, construct) = match function {
        ast::TsFnOrConstructorType::TsFnType(f) => (&f.params, &f.type_ann, false),
        ast::TsFnOrConstructorType::TsConstructorType(c) => (&c.params, &c.type_ann, true),
    };
    if signature.construct != construct {
        return Ok(false);
    }
    let mut position = 0;
    for param in params {
        match param {
            ast::TsFnParam::Rest(rest) => {
                let tuple = utilities::parameters_tuple(&signature, position, types)?;
                let Some(annotation) = &rest.type_ann else {
                    return Err(type_error("a rest parameter pattern requires a type"));
                };
                if has_infer(&annotation.type_ann) {
                    if !matches(&annotation.type_ann, tuple, bindings, types)? {
                        return Ok(false);
                    }
                } else if !never_array(&annotation.type_ann) {
                    // Contravariant: the pattern's list must be assignable
                    // to the source's (`never[]` is assignable to every list).
                    let wanted = with_bindings(&annotation.type_ann, bindings, types)?;
                    if !extends(wanted, tuple, types) {
                        return Ok(false);
                    }
                }
                position = signature.params.len();
            }
            ast::TsFnParam::Ident(ident) => {
                let Some(annotation) = &ident.type_ann else {
                    return Err(type_error("a parameter pattern requires a type"));
                };
                let Some(own) = signature.params.get(position).copied() else {
                    // A source with fewer parameters is assignable.
                    position += 1;
                    continue;
                };
                // Parameters are contravariant: the pattern's type must be
                // assignable to the source's (inferred positions bind it).
                if conditional_has_infer(&annotation.type_ann) {
                    if !matches(&annotation.type_ann, own, bindings, types)? {
                        return Ok(false);
                    }
                } else {
                    let wanted = with_bindings(&annotation.type_ann, bindings, types)?;
                    if !extends(wanted, own, types) {
                        return Ok(false);
                    }
                }
                position += 1;
            }
            _ => {
                return Err(unsupported(
                    "destructuring parameter in a function type pattern",
                ));
            }
        }
    }
    if position < signature.shape.minimum() {
        return Ok(false);
    }
    matches(&result.type_ann, signature.result, bindings, types)
}

fn conditional_has_infer(ty: &ast::TsType) -> bool {
    has_infer(ty)
}

/// `never[]`: the parameter list type every function's parameters accept.
fn never_array(ty: &ast::TsType) -> bool {
    matches!(ty, ast::TsType::TsArrayType(array)
        if matches!(&*array.elem_type, ast::TsType::TsKeywordType(k) if k.kind == ast::TsKeywordTypeKind::TsNeverKeyword))
}

/// `{ key: infer V }` against a record or class instance type.
fn object_pattern(
    literal: &ast::TsTypeLit,
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    for member in &literal.members {
        let ast::TsTypeElement::TsPropertySignature(property) = member else {
            return Err(unsupported(
                "infer inside a method or index signature pattern",
            ));
        };
        let name = records::expression_key(&property.key)?;
        let Some(annotation) = &property.type_ann else {
            return Err(type_error("a property pattern requires a type"));
        };
        let field = match ty {
            Type::Record(id) => types.records[id as usize]
                .fields
                .iter()
                .find(|field| field.name == name)
                .map(|field| (field.ty, field.optional)),
            Type::Class(class) => types
                .classes
                .field(class, &name)
                .map(|(_, f)| (f.ty, false)),
            _ => return Ok(false),
        };
        let Some((field_type, optional)) = field else {
            if property.optional {
                continue;
            }
            return Ok(false);
        };
        if optional && !property.optional {
            return Ok(false);
        }
        let field_type = if property.optional {
            tuple_types::strip_undefined(field_type, types)?
        } else {
            field_type
        };
        if !matches(&annotation.type_ann, field_type, bindings, types)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// `infer U | undefined` (and other unions with one inferring member): the
/// members of the check type assignable to the other pattern members are
/// removed and the inferring member matches the rest.
fn union_pattern(
    union: &ast::TsUnionType,
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let (inferring, plain): (Vec<_>, Vec<_>) =
        union.types.iter().partition(|member| has_infer(member));
    let [inferring] = inferring[..] else {
        return Err(unsupported(
            "union pattern with several inferring members (the matched member is ambiguous)",
        ));
    };
    let mut fixed = Vec::new();
    for member in plain {
        fixed.push(with_bindings(member, bindings, types)?);
    }
    let rest: Vec<Type> = split(ty, types)
        .into_iter()
        .filter(|member| !fixed.iter().any(|fixed| extends(*member, *fixed, types)))
        .collect();
    let rest = union_of(rest, types)?;
    matches(inferring, rest, bindings, types)
}

/// `Box<infer U>` for a declared generic type: its declaration body, with
/// every type parameter as an inference position, is matched against the
/// check type; each type argument pattern then matches what its parameter
/// bound (`unknown` when the body does not use it).
fn declared_pattern(
    name: &str,
    params: &[Box<ast::TsType>],
    ty: Type,
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    let Some((module, declaration, bodies)) = named::generic_declaration(types, name) else {
        return Err(unsupported(&format!(
            "infer inside the type arguments of {name}, which is not a generic interface or alias"
        )));
    };
    let names: Vec<String> = declaration
        .params
        .iter()
        .map(|param| param.name.sym.to_string())
        .collect();
    if params.len() > names.len() {
        return Ok(false);
    }
    let caller = named::current(types);
    named::enter(types, module);
    let mut inferred = generics::Frame::new();
    let mut matched = Ok(true);
    for body in &bodies {
        let pattern = inferable(body, &names);
        matched = matches(&pattern, ty, &mut inferred, types);
        if !matches!(matched, Ok(true)) {
            break;
        }
    }
    named::enter(types, caller);
    if !matched? {
        return Ok(false);
    }
    for (param, pattern) in names.iter().zip(params) {
        let bound = inferred
            .get(&inferable_name(param))
            .copied()
            .unwrap_or(Type::Unknown);
        if !matches(pattern, bound, bindings, types)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// The inference name standing for type parameter `name` in a declaration
/// body (`#` cannot start a source identifier).
fn inferable_name(name: &str) -> String {
    format!("#{name}")
}

/// `body` with every reference to one of `names` replaced by `infer #name`.
fn inferable(body: &ast::TsType, names: &[String]) -> ast::TsType {
    use swc_ecma_visit::{VisitMut, VisitMutWith};
    struct Replace<'a>(&'a [String]);
    impl VisitMut for Replace<'_> {
        fn visit_mut_ts_type(&mut self, ty: &mut ast::TsType) {
            if let ast::TsType::TsTypeRef(reference) = ty
                && reference.type_params.is_none()
                && let ast::TsEntityName::Ident(id) = &reference.type_name
                && self.0.iter().any(|name| *name == id.sym.as_ref())
            {
                *ty = ast::TsType::TsInferType(ast::TsInferType {
                    span: reference.span,
                    type_param: ast::TsTypeParam {
                        span: reference.span,
                        name: ast::Ident::new(
                            inferable_name(id.sym.as_ref()).into(),
                            id.span,
                            Default::default(),
                        ),
                        is_in: false,
                        is_out: false,
                        is_const: false,
                        constraint: None,
                        default: None,
                    },
                });
                return;
            }
            ty.visit_mut_children_with(self);
        }
    }
    let mut body = body.clone();
    body.visit_mut_with(&mut Replace(names));
    body
}

/// A rest pattern against a tuple with trailing elements (`[...string[],
/// number]`): the pattern's prefix must be the tuple's leading elements and
/// its suffix the trailing ones; the rest binds `[...R[]]`.
fn trailing_pattern(
    pattern: &ast::TsTupleType,
    source: &ir::TupleType,
    (rest_at, suffix): (usize, usize),
    bindings: &mut generics::Frame,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    if rest_at != source.elements.len() || suffix != source.trailing.len() {
        return Err(unsupported(
            "infer over a tuple with trailing elements by a pattern of another shape",
        ));
    }
    for (element, ty) in pattern.elem_types[..rest_at].iter().zip(&source.elements) {
        if !matches(&element.ty, *ty, bindings, types)? {
            return Ok(false);
        }
    }
    for (element, ty) in pattern.elem_types[rest_at + 1..]
        .iter()
        .zip(&source.trailing)
    {
        if !matches(&element.ty, *ty, bindings, types)? {
            return Ok(false);
        }
    }
    let middle = tuple_types::tuple_of(Vec::new(), 0, source.rest, types)?;
    let rest_pattern = match &*pattern.elem_types[rest_at].ty {
        ast::TsType::TsRestType(rest) => &*rest.type_ann,
        other => other,
    };
    matches(rest_pattern, middle, bindings, types)
}
