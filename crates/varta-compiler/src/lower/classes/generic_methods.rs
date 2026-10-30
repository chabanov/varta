//! Generic class methods (v37 `class_generics`), implemented by
//! monomorphization like generic top-level functions: every distinct
//! type-argument list of a method is one ordinary method function of its
//! owner (with the class receiver for instance methods), lowered in the
//! owner's module with the class context. An overridden generic method
//! dispatches through one synthesized dispatcher per root and type-argument
//! list, built after every instance exists, so the closed-world dispatch rule
//! of non-generic methods is unchanged.
use super::*;
use generics::{Frame, Missing};

/// Generic methods of one class by name.
pub(super) type GenericTable = BTreeMap<String, Rc<GenericMethod>>;

/// (owner class, method name, static, type arguments).
pub(super) type InstanceKey = (u32, String, bool, Vec<Type>);

/// A generic method instance whose body is not lowered yet.
pub(in crate::lower) struct Pending {
    owner: u32,
    pub(in crate::lower) module: usize,
    pub(in crate::lower) signature: Signature,
    frame: Frame,
    depth: usize,
    function: Rc<generics::Function>,
}

/// The diagnostic of a non-generic member named like a generic method.
pub(super) fn collision(
    id: u32,
    other: &GenericMethod,
    name: &str,
    method: &ast::ClassMethod,
    source: &Source,
) -> CompilerError {
    if other.owner == id {
        source.locate(
            invalid(format!("duplicate class member: {name}")),
            method.span,
        )
    } else {
        source.locate(
            type_error(format!(
                "override of {name} must keep the inherited method signature"
            )),
            method.key.span(),
        )
    }
}

/// Register a generic method of class `owner`. An instance method that
/// redeclares an inherited generic method overrides it and must keep its
/// accessibility and type-parameter count; the instantiated signatures are
/// compared per type-argument list. A static redeclaration shadows.
pub(super) fn declare(
    owner: u32,
    method: &ast::ClassMethod,
    inherited: &(GenericTable, GenericTable),
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<(String, GenericMethod)> {
    let unsupported_kind = if method.is_abstract {
        Some("abstract class method")
    } else if method.is_optional {
        Some("optional class method")
    } else if method.function.this_param.is_some() {
        Some("explicit this parameter on a class method")
    } else if !method.function.decorators.is_empty() {
        Some("decorated function")
    } else {
        None
    };
    if let Some(kind) = unsupported_kind {
        return Err(source.locate(unsupported(kind), method.span));
    }
    let name = member_name(&method.key).map_err(|e| source.locate(e, method.key.span()))?;
    if name == "toJSON" {
        return Err(source.locate(
            unsupported("class toJSON method; JSON serialization uses own fields"),
            method.key.span(),
        ));
    }
    let mut function = method.function.clone();
    let params = function
        .type_params
        .take()
        .ok_or_else(|| CompilerError::Internal("missing method type parameters".into()))?;
    let declaration = function_declaration(&name, method.key.span(), function);
    // Bodies are checked per instantiation; untyped code is rejected even in
    // a generic method that is never instantiated.
    if let Some(any) = generics::explicit_any(&declaration) {
        return Err(source.locate(unsupported("type annotation"), any));
    }
    let access = Access::from_ast(method.accessibility);
    let table = if method.is_static {
        &inherited.1
    } else {
        &inherited.0
    };
    let root = match table.get(&name) {
        Some(base) if base.owner == owner => {
            return Err(source.locate(
                invalid(format!("duplicate class member: {name}")),
                method.span,
            ));
        }
        Some(_) if method.is_static => owner,
        Some(base) => {
            if base.access == Access::Private || base.access != access {
                return Err(source.locate(
                    type_error(format!(
                        "override of {name} must keep the inherited non-private accessibility"
                    )),
                    method.key.span(),
                ));
            }
            if base.function.params.params.len() != params.params.len() {
                return Err(source.locate(
                    type_error(format!(
                        "override of {name} must keep the inherited type parameters"
                    )),
                    method.key.span(),
                ));
            }
            base.root
        }
        None => owner,
    };
    let class = &types.classes.list[owner as usize];
    let function = generics::Function {
        module: class.module,
        kind: "method",
        name: format!("{}.{name}", class.name),
        declaration,
        params: *params,
        source: source.clone(),
    };
    Ok((
        name,
        GenericMethod {
            owner,
            root,
            access,
            is_static: method.is_static,
            function: Rc::new(function),
        },
    ))
}

fn method_name(method: &GenericMethod) -> String {
    method.function.declaration.ident.sym.to_string()
}

/// The concrete type-argument list of `method` for the call's (explicit or
/// inferred) arguments, bound in the owner's module.
fn bind(
    method: &GenericMethod,
    args: &[Option<Type>],
    types: &mut ir::ValueTypes,
) -> Result<Frame> {
    let function = &method.function;
    let caller = named::current(types);
    named::enter(types, function.module);
    let what = format!("{} {}", function.kind, function.name);
    let frame = generics::bind(
        &function.params,
        args,
        &what,
        Missing::Inference,
        &function.source,
        types,
    );
    named::enter(types, caller);
    frame
}

fn key_of(method: &GenericMethod, frame: &Frame) -> Vec<Type> {
    method
        .function
        .params
        .params
        .iter()
        .map(|p| frame[p.name.sym.as_ref()])
        .collect()
}

/// The implementation instance of `method` for `key`, allocating its function
/// slot and queueing its body on first use.
fn instance(
    method: &GenericMethod,
    key: &[Type],
    next: &mut usize,
    types: &mut ir::ValueTypes,
) -> Result<Signature> {
    let args: Vec<Option<Type>> = key.iter().copied().map(Some).collect();
    let frame = bind(method, &args, types)?;
    let lookup = (
        method.owner,
        method_name(method),
        method.is_static,
        key.to_vec(),
    );
    if let Some(signature) = types.classes.method_instances.get(&lookup) {
        return Ok(signature.clone());
    }
    let function = &method.function;
    let depth = generics::depth(types) + 1;
    if depth > generics::MAX_INSTANTIATION_DEPTH {
        return Err(invalid(format!(
            "generic instantiation depth of method {} exceeds limit (polymorphic recursion?)",
            function.name
        )));
    }
    let index = *next;
    if index >= types.function_limit() {
        return Err(invalid(
            "too many functions including generic instantiations",
        ));
    }
    *next += 1;
    // A generic method of a generic class instance also sees the class's
    // type arguments (v37).
    let mut frame = frame;
    for (name, ty) in &types.classes.list[method.owner as usize].frame {
        frame.entry(name.clone()).or_insert(*ty);
    }
    let caller = named::current(types);
    named::enter(types, function.module);
    let checked = generics::with_frame(types, frame.clone(), |t| {
        signature::check(&function.declaration, index, &function.source, t)
    });
    named::enter(types, caller);
    let mut signature =
        checked.map_err(|e| function.source.locate(e, function.declaration.ident.span))?;
    if !method.is_static {
        signature.receiver = Some(Type::Class(method.owner));
        receiver_width(&signature)
            .map_err(|e| function.source.locate(e, function.declaration.ident.span))?;
    }
    types
        .classes
        .method_instances
        .insert(lookup, signature.clone());
    types.classes.pending_methods.push_back(Pending {
        owner: method.owner,
        module: function.module,
        signature: signature.clone(),
        frame,
        depth,
        function: Rc::clone(function),
    });
    Ok(signature)
}

/// Whether a descendant of `root` overrides its generic method `name`.
fn overridden(types: &ir::ValueTypes, root: u32, name: &str) -> bool {
    let classes = &types.classes;
    (0..classes.list.len() as u32).any(|id| {
        classes.descends(id, root)
            && classes.list[id as usize]
                .generic_methods
                .get(name)
                .is_some_and(|m| m.owner != root)
    })
}

/// The called function, its signature and receiver class for one call of
/// `method` with `args`: the owner's instance, or for an overridden instance
/// method (unless `direct`, as for `super.m()`) the dispatcher of the root's
/// instance, which instantiates every implementation with the same arguments.
fn instantiate(
    method: &GenericMethod,
    args: &[Option<Type>],
    direct: bool,
    next: &mut usize,
    types: &mut ir::ValueTypes,
) -> Result<(Signature, usize, u32)> {
    let frame = bind(method, args, types)?;
    let key = key_of(method, &frame);
    let name = method_name(method);
    let signature = instance(method, &key, next, types)?;
    if method.is_static || direct || !overridden(types, method.root, &name) {
        let index = signature.index;
        return Ok((signature, index, method.owner));
    }
    let root = Rc::clone(&types.classes.list[method.root as usize].generic_methods[&name]);
    let base = instance(&root, &key, next, types)?;
    let dispatcher = (method.root, name.clone(), key.clone());
    if let Some(index) = types.classes.generic_dispatchers.get(&dispatcher) {
        return Ok((base, *index, method.root));
    }
    let owners: BTreeSet<u32> = (0..types.classes.list.len() as u32)
        .filter(|id| types.classes.descends(*id, method.root))
        .map(|id| types.classes.list[id as usize].generic_methods[&name].owner)
        .collect();
    for owner in owners {
        let implementation = Rc::clone(&types.classes.list[owner as usize].generic_methods[&name]);
        let other = instance(&implementation, &key, next, types)?;
        if other.params != base.params || other.shape != base.shape || other.result != base.result {
            return Err(type_error(format!(
                "override of {name} must keep the inherited method signature \
                 (instantiated in {})",
                types.classes.list[owner as usize].name
            )));
        }
    }
    if *next >= types.function_limit() {
        return Err(invalid(
            "too many functions including generic instantiations",
        ));
    }
    let index = *next;
    *next += 1;
    types.classes.generic_dispatchers.insert(dispatcher, index);
    Ok((base, index, method.root))
}

/// Pop the next generic method instance whose body is pending.
pub(in crate::lower) fn next_generic_method(types: &mut ir::ValueTypes) -> Option<Pending> {
    types.classes.pending_methods.pop_front()
}

/// Lower a generic method instance in its owner's module with its frame and
/// the class context, like a non-generic method body.
pub(in crate::lower) fn generic_method_body(
    pending: Pending,
    (symbols, constants): (&link::Symbols, &module_constants::Scope),
    entry: usize,
    types: &mut ir::ValueTypes,
    synthesized: (
        &mut BTreeMap<usize, ir::Function>,
        &mut usize,
        &mut BTreeMap<usize, flow_calls::Impact>,
    ),
) -> Result<ir::Function> {
    let Pending {
        owner,
        module,
        signature,
        frame,
        depth,
        function,
    } = pending;
    named::enter(types, module);
    types.classes.context = Some(Context {
        class: owner,
        init: None,
    });
    let body = generics::at_depth(types, depth, |types| {
        generics::with_frame(types, frame, |types| {
            link::body(
                &function.declaration,
                (&signature, constants),
                symbols,
                &function.source,
                entry,
                types,
                synthesized,
            )
        })
    });
    types.classes.context = None;
    body
}

/// Synthesize the dispatcher of every overridden generic method instance
/// after all instances (and classes) exist.
pub(in crate::lower) fn generic_dispatch_bodies(
    types: &ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
) -> Result<()> {
    let classes = &types.classes;
    for ((root, name, key), index) in &classes.generic_dispatchers {
        let missing = || CompilerError::Internal("missing generic method instance".into());
        let base = classes
            .method_instances
            .get(&(*root, name.clone(), false, key.clone()))
            .ok_or_else(missing)?;
        let mut implementations: BTreeMap<(u32, usize), Vec<u32>> = BTreeMap::new();
        for id in 0..classes.list.len() as u32 {
            if classes.descends(id, *root) {
                let owner = classes.list[id as usize].generic_methods[name].owner;
                let instance = classes
                    .method_instances
                    .get(&(owner, name.clone(), false, key.clone()))
                    .ok_or_else(missing)?;
                implementations
                    .entry((owner, instance.index))
                    .or_default()
                    .push(classes.shape(id));
            }
        }
        generated.insert(
            *index,
            dispatch::dispatcher_function(types, *root, base, &implementations),
        );
    }
    Ok(())
}

impl Lower<'_> {
    /// `receiver.m<…>(…)`, `C.m<…>(…)` or `super.m<…>(…)` of a generic method:
    /// explicit type arguments, or arguments inferred from the call arguments
    /// and the contextual result type, select one instance. The receiver is
    /// already lowered; the arguments are lowered once, in source order.
    pub(in crate::lower) fn generic_method_call(
        &mut self,
        call: &ast::CallExpr,
        method: &Rc<GenericMethod>,
        receiver: Option<Expr>,
        direct: bool,
        expected: Option<Type>,
    ) -> Result<Expr> {
        let span = call.span;
        if call.args.iter().any(|arg| arg.spread.is_some()) {
            return Err(self
                .source
                .locate(unsupported("spread argument to a generic method"), span));
        }
        let function = Rc::clone(&method.function);
        let inferred = call.type_args.is_none();
        let (args, values) = if let Some(explicit) = &call.type_args {
            let explicit = generics::arguments(explicit, self.types)
                .map_err(|e| self.source.locate(e, explicit.span))?;
            (explicit.into_iter().map(Some).collect(), Vec::new())
        } else {
            self.infer(&function, call, expected)?
        };
        let (signature, target, class) =
            instantiate(method, &args, direct, self.next_function, self.types)
                .map_err(|e| self.source.locate(e, span))?;
        let values = self.instance_arguments(call, values, &signature)?;
        let mut all: Vec<Expr> = receiver
            .into_iter()
            .map(|value| Expr {
                ty: Type::Class(class),
                ..value
            })
            .collect();
        all.extend(self.bind_instance_arguments(values, &signature, &function, inferred, span)?);
        Ok(Expr {
            span,
            ty: signature.result,
            kind: E::Call(target, all),
        })
    }
}
