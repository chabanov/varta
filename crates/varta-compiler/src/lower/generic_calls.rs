//! Calls of generic top-level functions (v32 `generics()`). Explicit type
//! arguments, or arguments inferred from the call's argument types, select one
//! monomorphized instantiation. Arguments are lowered once, in source order.
use super::*;
use generics::Frame;
use std::collections::BTreeSet;
use swc_ecma_visit::{Visit, VisitWith};

/// Bound of nested type structure followed during inference.
const MAX_INFERENCE_DEPTH: usize = 64;

/// Type parameter names among `names` that a type expression mentions.
fn mentions(ty: &ast::TsType, names: &BTreeSet<String>) -> BTreeSet<String> {
    struct Names<'a> {
        names: &'a BTreeSet<String>,
        found: BTreeSet<String>,
    }
    impl Visit for Names<'_> {
        fn visit_ts_type_ref(&mut self, reference: &ast::TsTypeRef) {
            if let ast::TsEntityName::Ident(id) = &reference.type_name
                && self.names.contains(id.sym.as_ref())
            {
                self.found.insert(id.sym.to_string());
            }
            reference.visit_children_with(self);
        }
    }
    let mut visitor = Names {
        names,
        found: BTreeSet::new(),
    };
    ty.visit_with(&mut visitor);
    visitor.found
}

fn pattern_annotation(pattern: &ast::Pat) -> Option<&ast::TsType> {
    let annotation = match pattern {
        ast::Pat::Ident(binding) => binding.type_ann.as_deref(),
        ast::Pat::Assign(assign) => return pattern_annotation(&assign.left),
        ast::Pat::Array(array) => array.type_ann.as_deref(),
        ast::Pat::Object(object) => object.type_ann.as_deref(),
        _ => None,
    };
    annotation.map(|a| &*a.type_ann)
}

/// The declared type of the parameter receiving argument `index`; a rest
/// parameter contributes its element type.
fn parameter_annotation(params: &[ast::Param], index: usize) -> Option<&ast::TsType> {
    let rest = params
        .iter()
        .position(|p| matches!(p.pat, ast::Pat::Rest(_)));
    match rest {
        Some(position) if index >= position => {
            let ast::Pat::Rest(rest) = &params[position].pat else {
                return None;
            };
            match &*rest.type_ann.as_deref()?.type_ann {
                ast::TsType::TsArrayType(array) => Some(&array.elem_type),
                _ => None,
            }
        }
        _ => pattern_annotation(&params.get(index)?.pat),
    }
}

/// Run `lower` in the defining `module` of a declaration with `frame` visible.
fn in_definition<R>(
    module: usize,
    frame: &Frame,
    types: &mut ir::ValueTypes,
    lower: impl FnOnce(&mut ir::ValueTypes) -> R,
) -> R {
    let caller = named::current(types);
    named::enter(types, module);
    let result = generics::with_frame(types, frame.clone(), lower);
    named::enter(types, caller);
    result
}

struct Inference {
    /// Module of the generic declaration whose parameters are inferred.
    module: usize,
    /// The declaration, for diagnostics.
    owner: String,
    names: BTreeSet<String>,
    bound: Frame,
}
impl Inference {
    /// Match a parameter type expression against an argument type, binding the
    /// type parameters it determines. Positions that determine nothing
    /// (unions with several generic members, named generic types) are skipped.
    fn unify(
        &mut self,
        pattern: &ast::TsType,
        actual: Type,
        types: &mut ir::ValueTypes,
        depth: usize,
    ) -> Result<()> {
        if depth > MAX_INFERENCE_DEPTH {
            return Ok(());
        }
        match pattern {
            ast::TsType::TsParenthesizedType(inner) => {
                self.unify(&inner.type_ann, actual, types, depth + 1)
            }
            ast::TsType::TsTypeRef(reference)
                if reference.type_params.is_none()
                    && matches!(&reference.type_name, ast::TsEntityName::Ident(id)
                        if self.names.contains(id.sym.as_ref())) =>
            {
                let ast::TsEntityName::Ident(id) = &reference.type_name else {
                    return Ok(());
                };
                self.bind(id.sym.as_ref(), actual, types)
            }
            ast::TsType::TsArrayType(array) => self.element(&array.elem_type, actual, types, depth),
            ast::TsType::TsTypeOperator(operator)
                if operator.op == ast::TsTypeOperatorOp::ReadOnly =>
            {
                self.unify(&operator.type_ann, actual, types, depth + 1)
            }
            ast::TsType::TsTypeRef(reference)
                if matches!(&reference.type_name, ast::TsEntityName::Ident(id)
                if matches!(id.sym.as_ref(), "Array" | "ReadonlyArray")
                    && !in_definition(self.module, &Frame::new(), types, |t| {
                        named::declared(t, id.sym.as_ref())
                    })) =>
            {
                match reference.type_params.as_deref().map(|p| &p.params[..]) {
                    Some([element]) => self.element(element, actual, types, depth),
                    _ => Ok(()),
                }
            }
            ast::TsType::TsUnionOrIntersectionType(
                ast::TsUnionOrIntersectionType::TsUnionType(union),
            ) => self.union(union, actual, types, depth),
            ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsFnType(callback)) => {
                let Type::Function(id) = actual else {
                    return Ok(());
                };
                let signature = types.functions[id as usize].clone();
                for (parameter, ty) in callback.params.iter().zip(signature.params) {
                    if let ast::TsFnParam::Ident(binding) = parameter
                        && let Some(annotation) = &binding.type_ann
                    {
                        self.unify(&annotation.type_ann, ty, types, depth + 1)?;
                    }
                }
                self.unify(
                    &callback.type_ann.type_ann,
                    signature.result,
                    types,
                    depth + 1,
                )
            }
            // `{ [key: string]: T }` (v35 `mixed_index_records`).
            ast::TsType::TsTypeLit(literal)
                if types.profile.mixed_index_records()
                    && let [ast::TsTypeElement::TsIndexSignature(index)] =
                        literal.members.as_slice()
                    && let Some(value) = index.type_ann.as_deref() =>
            {
                self.index_value(&value.type_ann, actual, types, depth)
            }
            ast::TsType::TsTypeRef(reference)
                if types.profile.mixed_index_records()
                    && matches!(&reference.type_name, ast::TsEntityName::Ident(id)
                    if id.sym == "Record"
                        && !in_definition(self.module, &Frame::new(), types, |t| {
                            named::declared(t, "Record")
                        }))
                    && let Some([key, value]) =
                        reference.type_params.as_deref().map(|p| &p.params[..])
                    && matches!(&**key, ast::TsType::TsKeywordType(k)
                        if k.kind == ast::TsKeywordTypeKind::TsStringKeyword) =>
            {
                self.index_value(value, actual, types, depth)
            }
            ast::TsType::TsTypeLit(literal) => {
                let Type::Record(id) = actual else {
                    return Ok(());
                };
                for member in &literal.members {
                    let ast::TsTypeElement::TsPropertySignature(property) = member else {
                        continue;
                    };
                    let (Ok(name), Some(annotation)) = (
                        records::expression_key(&property.key),
                        property.type_ann.as_deref(),
                    ) else {
                        continue;
                    };
                    let Some(field) = types.records[id as usize]
                        .fields
                        .iter()
                        .find(|f| f.name == name)
                        .cloned()
                    else {
                        continue;
                    };
                    let ty = if field.optional {
                        without_undefined(field.ty, types)?
                    } else {
                        field.ty
                    };
                    self.unify(&annotation.type_ann, ty, types, depth + 1)?;
                }
                Ok(())
            }
            ast::TsType::TsTypeRef(reference) => self.named(reference, actual, types, depth),
            _ => Ok(()),
        }
    }
    /// A reference to a generic named type: infer its own parameters from the
    /// declaration body, then match its type arguments against them.
    fn named(
        &mut self,
        reference: &ast::TsTypeRef,
        actual: Type,
        types: &mut ir::ValueTypes,
        depth: usize,
    ) -> Result<()> {
        let (ast::TsEntityName::Ident(id), Some(args)) =
            (&reference.type_name, reference.type_params.as_deref())
        else {
            return Ok(());
        };
        let Some((module, params, bodies)) =
            in_definition(self.module, &Frame::new(), types, |t| {
                named::generic_declaration(t, id.sym.as_ref())
            })
        else {
            return Ok(());
        };
        let mut inner = Inference {
            module,
            owner: format!("type {}", id.sym),
            names: params
                .params
                .iter()
                .map(|p| p.name.sym.to_string())
                .collect(),
            bound: Frame::new(),
        };
        for body in &bodies {
            inner.unify(body, actual, types, depth + 1)?;
        }
        for (parameter, arg) in params.params.iter().zip(&args.params) {
            if let Some(ty) = inner.bound.get(parameter.name.sym.as_ref()).copied() {
                self.unify(arg, ty, types, depth + 1)?;
            }
        }
        Ok(())
    }
    /// The value type of a `Record<string, T>` or index signature pattern:
    /// a dictionary argument's value type, or the union of the field types of
    /// a record argument (an object literal), with literal types widened.
    fn index_value(
        &mut self,
        pattern: &ast::TsType,
        actual: Type,
        types: &mut ir::ValueTypes,
        depth: usize,
    ) -> Result<()> {
        let value = match actual {
            Type::Dictionary(id) => types.dictionaries[id as usize].value,
            Type::Record(id) if types.records[id as usize].class.is_none() => {
                let fields: Vec<_> = types.records[id as usize]
                    .fields
                    .iter()
                    .map(|field| field.ty.erased())
                    .collect();
                if fields.is_empty() {
                    return Ok(());
                }
                unions::intern(fields, types)?
            }
            _ => return Ok(()),
        };
        self.unify(pattern, value, types, depth + 1)
    }
    fn element(
        &mut self,
        pattern: &ast::TsType,
        actual: Type,
        types: &mut ir::ValueTypes,
        depth: usize,
    ) -> Result<()> {
        if let Type::Array(id) = actual {
            let element = types.arrays[id as usize];
            self.unify(pattern, element, types, depth + 1)?;
        }
        Ok(())
    }
    /// `T | undefined`-style positions: the argument members not covered by
    /// the concrete members determine the single generic member.
    fn union(
        &mut self,
        union: &ast::TsUnionType,
        actual: Type,
        types: &mut ir::ValueTypes,
        depth: usize,
    ) -> Result<()> {
        let (generic, concrete): (Vec<_>, Vec<_>) = union
            .types
            .iter()
            .partition(|member| !mentions(member, &self.names).is_empty());
        let [generic] = generic[..] else {
            return Ok(());
        };
        let mut covered = Vec::new();
        for member in concrete {
            match in_definition(self.module, &Frame::new(), types, |t| {
                keyword_type(member, t)
            }) {
                Ok(ty) => covered.push(ty),
                Err(_) => return Ok(()),
            }
        }
        let remaining: Vec<Type> = unions::members(actual, types)
            .into_iter()
            .filter(|member| {
                !covered
                    .iter()
                    .any(|c| arguments::accepts(*c, *member, types))
            })
            .collect();
        match remaining[..] {
            [] => Ok(()),
            [single] => self.unify(generic, single, types, depth + 1),
            _ => {
                let rest = unions::intern(remaining, types)?;
                self.unify(generic, rest, types, depth + 1)
            }
        }
    }
    fn bind(&mut self, name: &str, actual: Type, types: &ir::ValueTypes) -> Result<()> {
        match self.bound.get(name).copied() {
            None => {
                self.bound.insert(name.to_owned(), actual);
            }
            Some(previous) if arguments::accepts(previous, actual, types) => {}
            Some(previous) if arguments::accepts(actual, previous, types) => {
                self.bound.insert(name.to_owned(), actual);
            }
            Some(previous) => {
                return Err(type_error(format!(
                    "conflicting inferences {previous} and {actual} for type parameter {name} \
                     of {}; provide explicit type arguments",
                    self.owner
                )));
            }
        }
        Ok(())
    }
}

fn without_undefined(ty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    let members: Vec<_> = unions::members(ty, types)
        .into_iter()
        .filter(|m| *m != Type::Undefined)
        .collect();
    match members[..] {
        [single] => Ok(single),
        _ => unions::intern(members, types),
    }
}

impl Lower<'_> {
    /// The generic function a call names, unless a lexical binding or an
    /// ordinary function of the same name shadows it.
    pub(super) fn generic_callee(&self, call: &ast::CallExpr) -> Option<usize> {
        if !self.types.profile.generics() {
            return None;
        }
        let ast::Callee::Expr(callee) = &call.callee else {
            return None;
        };
        let ast::Expr::Ident(id) = &**callee else {
            return None;
        };
        self.generic_name(id.sym.as_ref())
    }
    pub(super) fn generic_name(&self, name: &str) -> Option<usize> {
        if !self.types.profile.generics()
            || self.binding(name).is_ok()
            || self.functions.contains_key(name)
        {
            return None;
        }
        generics::lookup(self.types, named::current(self.types), name)
    }
    pub(super) fn generic_call(
        &mut self,
        call: &ast::CallExpr,
        id: usize,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = call.span;
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self
                .source
                .locate(unsupported("spread argument to a generic function"), span));
        }
        let function = generics::function(self.types, id);
        let inferred = call.type_args.is_none();
        let (args, mut values) = if let Some(explicit) = &call.type_args {
            let explicit = generics::arguments(explicit, self.types)
                .map_err(|e| self.source.locate(e, explicit.span))?;
            (explicit.into_iter().map(Some).collect(), Vec::new())
        } else {
            self.infer(&function, call, expected)?
        };
        let signature = generics::instantiate(id, &args, self.next_function, self.types)
            .map_err(|e| self.source.locate(e, span))?;
        arguments::count(call.args.len(), &signature.shape)
            .map_err(|e| self.source.locate(e, span))?;
        for (index, arg) in call.args.iter().enumerate().skip(values.len()) {
            let context =
                arguments::context(index, &signature.params, &signature.shape, self.types);
            values.push(self.expr_expected(&arg.expr, context)?);
        }
        // An object literal typed on its own while it determined the value
        // type of a `Record<string, T>` parameter (v35) is copied into the
        // instantiated dictionary, keeping its key order.
        if self.types.profile.mixed_index_records() {
            for (value, &param) in values.iter_mut().zip(&signature.params) {
                if matches!(param, Type::Dictionary(_)) && matches!(value.ty, Type::Record(_)) {
                    let record = std::mem::replace(
                        value,
                        Expr {
                            span,
                            ty: Type::Undefined,
                            kind: E::Integer(0),
                        },
                    );
                    *value = self.dictionary_from_record(record, param)?;
                }
            }
        }
        let args = self
            .bind_arguments(values, &signature.params, &signature.shape, span)
            .map_err(|error| match error.diagnostic() {
                // An argument typed on its own (without context) fixed the
                // instantiation; say so instead of only reporting the mismatch.
                Some(diagnostic) if inferred => self.source.locate(
                    type_error(format!(
                        "{} (type arguments of function {} were inferred from the arguments \
                         without contextual typing; provide explicit type arguments)",
                        diagnostic.message, function.name
                    )),
                    span,
                ),
                _ => error,
            })?;
        Ok(Expr {
            span,
            ty: signature.result,
            kind: E::Call(signature.index, args),
        })
    }
    /// Lower every argument in source order: against its parameter type when
    /// that type is already determined, otherwise on its own (or against the
    /// constraint of a bare type parameter) and unify the result.
    fn infer(
        &mut self,
        function: &generics::Function,
        call: &ast::CallExpr,
        expected: Option<Type>,
    ) -> Result<(Vec<Option<Type>>, Vec<Expr>)> {
        let mut inference = Inference {
            module: function.module,
            owner: format!("function {}", function.name),
            names: function
                .params
                .params
                .iter()
                .map(|p| p.name.sym.to_string())
                .collect(),
            bound: Frame::new(),
        };
        let params = &function.declaration.function.params;
        let mut values = Vec::new();
        for (index, arg) in call.args.iter().enumerate() {
            let Some(pattern) = parameter_annotation(params, index) else {
                values.push(self.expr(&arg.expr)?);
                continue;
            };
            let mentioned = mentions(pattern, &inference.names);
            let value = if mentioned.iter().all(|n| inference.bound.contains_key(n)) {
                let ty = in_definition(function.module, &inference.bound, self.types, |t| {
                    keyword_type(pattern, t)
                })
                .map_err(|e| function.source.locate(e, pattern.span()))?;
                self.expr_expected(&arg.expr, Some(ty))?
            } else {
                let hint = self.constraint_hint(function, &inference.bound, pattern);
                let value = self.expr_expected(&arg.expr, hint)?;
                inference
                    .unify(pattern, value.ty, self.types, 0)
                    .map_err(|e| self.source.locate(e, value.span))?;
                value
            };
            values.push(value);
        }
        // A contextual result type determines parameters the arguments did not.
        if let Some(expected) = expected
            && let Some(result) = &function.declaration.function.return_type
        {
            let mut context = Inference {
                module: function.module,
                owner: inference.owner.clone(),
                names: inference.names.clone(),
                bound: inference.bound.clone(),
            };
            if context
                .unify(&result.type_ann, expected, self.types, 0)
                .is_ok()
            {
                for (name, ty) in context.bound {
                    inference.bound.entry(name).or_insert(ty);
                }
            }
        }
        let args = function
            .params
            .params
            .iter()
            .map(|p| inference.bound.get(p.name.sym.as_ref()).copied())
            .collect();
        Ok((args, values))
    }
    /// For an argument whose parameter is a bare type parameter, its
    /// constraint (over bound parameters) as the argument's contextual type.
    fn constraint_hint(
        &mut self,
        function: &generics::Function,
        bound: &Frame,
        pattern: &ast::TsType,
    ) -> Option<Type> {
        let ast::TsType::TsTypeRef(reference) = pattern else {
            return None;
        };
        let ast::TsEntityName::Ident(id) = &reference.type_name else {
            return None;
        };
        let parameter = function
            .params
            .params
            .iter()
            .find(|p| p.name.sym == id.sym)?;
        in_definition(function.module, &Frame::new(), self.types, |t| {
            generics::constraint_hint(parameter, bound, t)
        })
    }
}
