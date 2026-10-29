//! Generic interfaces, aliases and top-level functions (v32 `generics()`),
//! implemented by monomorphization. Type arguments are ordinary interned types:
//! a generic interface instance is the same structural shape as the equivalent
//! type literal, and every distinct instantiation of a generic function is one
//! emitted function. Nothing generic reaches emission.
use super::*;
use crate::modules::Graph;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;

/// Nesting bound of generic function instantiations requested from the bodies
/// of other instantiations. Polymorphic recursion (`f<T[]>` inside `f<T>`)
/// would otherwise only stop at the function or array type limits.
pub(crate) const MAX_INSTANTIATION_DEPTH: usize = 32;

/// Type parameters bound in one declaration, by name.
pub(super) type Frame = BTreeMap<String, Type>;

/// A generic top-level function: the declaration without its type parameter
/// list, lowered once per instantiation in its defining module.
pub(super) struct Function {
    pub(super) module: usize,
    pub(super) name: String,
    pub(super) declaration: ast::FnDecl,
    pub(super) params: ast::TsTypeParamDecl,
    pub(super) source: Source,
}

#[derive(Clone)]
struct Instance {
    signature: Signature,
    frame: Frame,
    depth: usize,
}

#[derive(Default)]
pub(crate) struct Generics {
    /// Type-parameter frames. Only the innermost frame is visible: generic
    /// declarations are top-level, so type parameters never nest lexically.
    frames: Vec<Frame>,
    functions: Vec<Rc<Function>>,
    /// Module-local and imported generic function bindings.
    bindings: BTreeMap<(usize, String), usize>,
    instances: BTreeMap<(usize, Vec<Type>), Instance>,
    pending: VecDeque<(usize, Vec<Type>)>,
    /// Instantiation depth of the body being lowered (0 outside instances).
    depth: usize,
}

/// A reference to a type parameter visible in the current frame.
pub(super) fn parameter(ty: &ast::TsType, types: &ir::ValueTypes) -> Option<Type> {
    let ast::TsType::TsTypeRef(reference) = ty else {
        return None;
    };
    let ast::TsEntityName::Ident(id) = &reference.type_name else {
        return None;
    };
    if reference.type_params.is_some() {
        return None;
    }
    types.generics.frames.last()?.get(id.sym.as_ref()).copied()
}

pub(super) fn with_frame<R>(
    types: &mut ir::ValueTypes,
    frame: Frame,
    lower: impl FnOnce(&mut ir::ValueTypes) -> R,
) -> R {
    types.generics.frames.push(frame);
    let result = lower(types);
    types.generics.frames.pop();
    result
}

/// Lower explicit type arguments in the referencing module and frame.
pub(super) fn arguments(
    instantiation: &ast::TsTypeParamInstantiation,
    types: &mut ir::ValueTypes,
) -> Result<Vec<Type>> {
    instantiation
        .params
        .iter()
        .map(|ty| keyword_type(ty, types))
        .collect()
}

/// How a missing type argument is reported.
#[derive(Clone, Copy)]
pub(super) enum Missing {
    /// A generic type reference must list every parameter without a default.
    Reference,
    /// A generic call whose arguments did not determine the parameter.
    Inference,
}

/// Bind the type parameters of one declaration in its defining module:
/// supplied arguments, then defaults (which may name earlier parameters), each
/// checked against its `extends` constraint. Constraint and arity errors are
/// left unlocated for the referencing site.
pub(super) fn bind(
    declaration: &ast::TsTypeParamDecl,
    args: &[Option<Type>],
    what: &str,
    missing: Missing,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<Frame> {
    if args.len() > declaration.params.len() {
        return Err(type_error(format!(
            "{what} expects at most {} type argument(s), found {}",
            declaration.params.len(),
            args.len()
        )));
    }
    let mut frame = Frame::new();
    for (position, parameter) in declaration.params.iter().enumerate() {
        if parameter.is_in || parameter.is_out || parameter.is_const {
            return Err(source.locate(
                unsupported("in/out/const type parameter modifier"),
                parameter.span,
            ));
        }
        let name = parameter.name.sym.to_string();
        let ty = match (args.get(position).copied().flatten(), &parameter.default) {
            (Some(ty), _) => ty,
            (None, Some(default)) => with_frame(types, frame.clone(), |t| keyword_type(default, t))
                .map_err(|e| source.locate(e, default.span()))?,
            (None, None) => {
                return Err(type_error(match missing {
                    Missing::Reference => {
                        format!("{what} requires a type argument for {name}")
                    }
                    Missing::Inference => format!(
                        "cannot infer type argument {name} of {what} from the call arguments; \
                         provide explicit type arguments"
                    ),
                }));
            }
        };
        let mut scope = frame.clone();
        scope.insert(name.clone(), ty);
        if let Some(constraint) = &parameter.constraint
            && !satisfies(constraint, ty, &scope, source, types)?
        {
            return Err(type_error(format!(
                "type argument {ty} for {name} of {what} does not satisfy its constraint"
            )));
        }
        if frame.insert(name.clone(), ty).is_some() {
            return Err(source.locate(
                invalid(format!("duplicate type parameter: {name}")),
                parameter.name.span,
            ));
        }
    }
    Ok(frame)
}

/// Whether `ty` satisfies an `extends` constraint. `object` admits the
/// non-primitive kinds; every other constraint is an admitted type expression
/// checked with the ordinary argument assignability.
fn satisfies(
    constraint: &ast::TsType,
    ty: Type,
    scope: &Frame,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<bool> {
    if let ast::TsType::TsKeywordType(keyword) = constraint {
        match keyword.kind {
            ast::TsKeywordTypeKind::TsObjectKeyword => {
                return Ok(unions::members(ty, types).iter().all(|member| {
                    matches!(
                        member,
                        Type::Record(_) | Type::Class(_) | Type::Array(_) | Type::Function(_)
                    )
                }));
            }
            ast::TsKeywordTypeKind::TsUnknownKeyword => return Ok(true),
            _ => {}
        }
    }
    let bound = with_frame(types, scope.clone(), |t| keyword_type(constraint, t))
        .map_err(|e| source.locate(e, constraint.span()))?;
    Ok(arguments::accepts(bound, ty, types))
}

/// The constraint of a type parameter as a contextual type for an argument,
/// when it is an admitted type expression over already bound parameters.
pub(super) fn constraint_hint(
    parameter: &ast::TsTypeParam,
    scope: &Frame,
    types: &mut ir::ValueTypes,
) -> Option<Type> {
    let constraint = parameter.constraint.as_deref()?;
    if matches!(constraint, ast::TsType::TsKeywordType(_)) {
        return None;
    }
    with_frame(types, scope.clone(), |t| keyword_type(constraint, t)).ok()
}

/// Lower the body of a named declaration with its bound type parameters.
pub(super) fn declaration_body(
    frame: Frame,
    body: &ast::TsType,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    with_frame(types, frame, |t| keyword_type(body, t))
}

/// Register a generic top-level function of `module` (it gets no function
/// slot; instantiations do).
pub(super) fn declare(
    declaration: &ast::FnDecl,
    module: usize,
    entry: bool,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<()> {
    let function = &declaration.function;
    let span = declaration.ident.span;
    let name = declaration.ident.sym.to_string();
    let rejected = if function.is_async || function.is_generator {
        Some("async or generator generic function")
    } else if function.this_param.is_some() {
        Some("generic function with a this parameter")
    } else if !function.decorators.is_empty() {
        Some("decorated function")
    } else {
        None
    };
    if let Some(kind) = rejected {
        return Err(source.locate(unsupported(kind), span));
    }
    if entry && name == "main" {
        return Err(source.locate(invalid("main must have signature () -> void"), span));
    }
    // Bodies are checked per instantiation; untyped code is rejected even in
    // a generic function that is never instantiated.
    if let Some(any) = explicit_any(declaration) {
        return Err(source.locate(unsupported("type annotation"), any));
    }
    let mut stripped = declaration.clone();
    let params = stripped
        .function
        .type_params
        .take()
        .ok_or_else(|| invalid("missing type parameters"))?;
    let id = types.generics.functions.len();
    if matches!(name.as_str(), "varta" | "console" | "log" | "JSON")
        || types
            .generics
            .bindings
            .insert((module, name.clone()), id)
            .is_some()
    {
        return Err(source.locate(
            invalid(format!("duplicate or reserved function name: {name}")),
            span,
        ));
    }
    types.generics.functions.push(Rc::new(Function {
        module,
        name,
        declaration: stripped,
        params: *params,
        source: source.clone(),
    }));
    Ok(())
}

fn explicit_any(declaration: &ast::FnDecl) -> Option<swc_common::Span> {
    use swc_ecma_visit::{Visit, VisitWith};
    struct Any(Option<swc_common::Span>);
    impl Visit for Any {
        fn visit_ts_keyword_type(&mut self, keyword: &ast::TsKeywordType) {
            if keyword.kind == ast::TsKeywordTypeKind::TsAnyKeyword {
                self.0.get_or_insert(keyword.span);
            }
        }
    }
    let mut visitor = Any(None);
    declaration.visit_with(&mut visitor);
    visitor.0
}

/// A generic function of `module` named like a class of its type namespace.
pub(super) fn class_collision(types: &ir::ValueTypes, module: usize) -> Option<String> {
    types
        .generics
        .bindings
        .keys()
        .filter(|(owner, _)| *owner == module)
        .map(|(_, name)| name)
        .find(|name| named::class(types, name).is_some())
        .cloned()
}

/// The generic function bound to `name` in `module`, if any.
pub(super) fn lookup(types: &ir::ValueTypes, module: usize, name: &str) -> Option<usize> {
    types
        .generics
        .bindings
        .get(&(module, name.to_owned()))
        .copied()
}

/// Bind an imported generic function; false for a duplicate local name.
pub(super) fn import(types: &mut ir::ValueTypes, module: usize, local: &str, id: usize) -> bool {
    types
        .generics
        .bindings
        .insert((module, local.to_owned()), id)
        .is_none()
}

/// A name of `module` that is both a generic function and in `names`.
pub(super) fn collision<'a>(
    types: &ir::ValueTypes,
    module: usize,
    names: impl IntoIterator<Item = &'a String>,
) -> Option<&'a String> {
    names
        .into_iter()
        .find(|name| lookup(types, module, name).is_some())
}

pub(super) fn function(types: &ir::ValueTypes, id: usize) -> Rc<Function> {
    Rc::clone(&types.generics.functions[id])
}

/// Signature of the instantiation of generic function `id` with `args`
/// (missing entries use defaults), allocating its function slot on first use.
pub(super) fn instantiate(
    id: usize,
    args: &[Option<Type>],
    next_function: &mut usize,
    types: &mut ir::ValueTypes,
) -> Result<Signature> {
    let function = function(types, id);
    let caller = named::current(types);
    named::enter(types, function.module);
    let result = instance(&function, id, args, next_function, types);
    named::enter(types, caller);
    result
}

fn instance(
    function: &Function,
    id: usize,
    args: &[Option<Type>],
    next_function: &mut usize,
    types: &mut ir::ValueTypes,
) -> Result<Signature> {
    let what = format!("function {}", function.name);
    let frame = bind(
        &function.params,
        args,
        &what,
        Missing::Inference,
        &function.source,
        types,
    )?;
    let key: Vec<Type> = function
        .params
        .params
        .iter()
        .map(|p| frame[p.name.sym.as_ref()])
        .collect();
    if let Some(instance) = types.generics.instances.get(&(id, key.clone())) {
        return Ok(instance.signature.clone());
    }
    let depth = types.generics.depth + 1;
    if depth > MAX_INSTANTIATION_DEPTH {
        return Err(invalid(format!(
            "generic instantiation depth of {what} exceeds limit (polymorphic recursion?)"
        )));
    }
    let index = *next_function;
    if index >= varta_common::abi::MAX_USER_FUNCTIONS {
        return Err(invalid(
            "too many functions including generic instantiations",
        ));
    }
    *next_function += 1;
    let signature = with_frame(types, frame.clone(), |t| {
        signature::check(&function.declaration, index, &function.source, t)
    })
    .map_err(|e| function.source.locate(e, function.declaration.ident.span))?;
    types.generics.instances.insert(
        (id, key.clone()),
        Instance {
            signature: signature.clone(),
            frame,
            depth,
        },
    );
    types.generics.pending.push_back((id, key));
    Ok(signature)
}

/// Lower every requested instantiation in its defining module, including the
/// ones requested by other instantiations, into the generated function table.
pub(super) fn bodies(
    graph: &Graph,
    scopes: &BTreeMap<PathBuf, link::Symbols>,
    constants: &BTreeMap<PathBuf, module_constants::Scope>,
    entry: usize,
    types: &mut ir::ValueTypes,
    synthesized: (
        &mut BTreeMap<usize, ir::Function>,
        &mut usize,
        &mut BTreeMap<usize, flow_calls::Impact>,
    ),
) -> Result<()> {
    let (generated, next, call_effects) = synthesized;
    let empty = module_constants::Scope::new();
    while let Some(key) = types.generics.pending.pop_front() {
        let instance = types.generics.instances[&key].clone();
        let function = function(types, key.0);
        let path = &graph.order[function.module];
        named::enter(types, function.module);
        types.generics.depth = instance.depth;
        let visible = constants.get(path).unwrap_or(&empty);
        let body = with_frame(types, instance.frame, |types| {
            link::body(
                &function.declaration,
                (&instance.signature, visible),
                &scopes[path],
                &function.source,
                entry,
                types,
                (&mut *generated, &mut *next, &mut *call_effects),
            )
        });
        types.generics.depth = 0;
        generated.insert(instance.signature.index, body?);
    }
    Ok(())
}
