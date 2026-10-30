//! Class function bodies: constructor initializers and methods are lowered
//! like declarations with a class receiver; dispatchers are synthesized IR.
use super::*;

/// After every module is laid out, give each overridden root method one
/// dispatcher shared by the root class and all of its descendants.
pub(in crate::lower) fn dispatchers(types: &mut ir::ValueTypes, next: &mut usize) -> Result<()> {
    // Generic class instances (v37) created before every declared class was
    // laid out are laid out now; later ones lay out when they are created.
    generic_classes::lay_out_deferred(types)?;
    let classes = &types.classes;
    let mut roots = Vec::new();
    for (id, class) in classes.list.iter().enumerate() {
        let id = id as u32;
        for (name, method) in &class.methods {
            if method.owner != id || method.root != id {
                continue;
            }
            let overridden = (0..classes.list.len() as u32).any(|other| {
                other != id
                    && classes.descends(other, id)
                    && classes.list[other as usize].methods[name].owner != id
            });
            if overridden {
                roots.push((id, name.clone()));
            }
        }
    }
    for (root, name) in roots {
        if *next >= types.function_limit() {
            let class = &types.classes.list[root as usize];
            return Err(class
                .source
                .locate(invalid("too many functions across modules"), class.span));
        }
        let index = *next;
        *next += 1;
        for other in 0..types.classes.list.len() as u32 {
            if types.classes.descends(other, root) {
                let method = types.classes.list[other as usize]
                    .methods
                    .get_mut(&name)
                    .ok_or_else(|| CompilerError::Internal("missing inherited method".into()))?;
                method.dispatch = Some(index);
            }
        }
        types
            .classes
            .dispatchers
            .push(Dispatcher { root, name, index });
    }
    // Instance member functions take slots after the dispatchers.
    generic_classes::materialize(types, next)
}

/// Lower the initializer and method bodies of the classes of `module` in the
/// function-index order assigned by `layout_module`. Bodies read the module's
/// constants (v32 `module_constants`) like its function bodies do.
pub(in crate::lower) fn bodies(
    module: usize,
    (symbols, constants): (&link::Symbols, &module_constants::Scope),
    entry: usize,
    types: &mut ir::ValueTypes,
    functions: &mut Vec<ir::Function>,
    synthesized: (
        &mut BTreeMap<usize, ir::Function>,
        &mut usize,
        &mut BTreeMap<usize, flow_calls::Impact>,
    ),
) -> Result<()> {
    let (generated, next, effects) = synthesized;
    let order: Vec<_> = types
        .classes
        .order
        .iter()
        .filter(|(_, class, _)| types.classes.list[*class as usize].module == module)
        .cloned()
        .collect();
    for (index, class, member) in order {
        if functions.len() != index {
            return Err(CompilerError::Internal("class function order".into()));
        }
        functions.push(member_function(
            (index, class, &member),
            (symbols, constants),
            entry,
            types,
            (&mut *generated, &mut *next, &mut *effects),
        )?);
    }
    Ok(())
}

/// Lower one initializer or method body of `class`; a generic class
/// instance's body sees its type arguments.
pub(super) fn member_function(
    (index, class, member): (usize, u32, &Member),
    (symbols, constants): (&link::Symbols, &module_constants::Scope),
    entry: usize,
    types: &mut ir::ValueTypes,
    synthesized: (
        &mut BTreeMap<usize, ir::Function>,
        &mut usize,
        &mut BTreeMap<usize, flow_calls::Impact>,
    ),
) -> Result<ir::Function> {
    {
        let (declaration, signature, init) = match member {
            Member::Init => {
                let declaration = constructors::declaration(class, types)?;
                let signature = types.classes.list[class as usize]
                    .init
                    .clone()
                    .ok_or_else(|| CompilerError::Internal("missing class initializer".into()))?;
                (declaration, signature, Some(index))
            }
            Member::Method(name) | Member::Static(name) => {
                let is_static = matches!(member, Member::Static(_));
                let info = &types.classes.list[class as usize];
                let method = info
                    .declaration
                    .body
                    .iter()
                    .find_map(|member| match member {
                        ast::ClassMember::Method(m)
                            if m.is_static == is_static
                                && method_name(&m.key, types.profile, true)
                                    .is_ok_and(|n| &n == name) =>
                        {
                            Some(m)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| CompilerError::Internal("missing class method".into()))?;
                let declaration =
                    function_declaration(name, method.key.span(), method.function.clone());
                let table = if is_static {
                    &info.statics
                } else {
                    &info.methods
                };
                let signature = table
                    .get(name)
                    .ok_or_else(|| CompilerError::Internal("missing class method".into()))?
                    .signature
                    .clone();
                (declaration, signature, None)
            }
        };
        let source = types.classes.list[class as usize].source.clone();
        let frame = types.classes.list[class as usize].frame.clone();
        types.classes.context = Some(Context { class, init });
        let lower = |types: &mut ir::ValueTypes| {
            link::body(
                &declaration,
                (&signature, constants),
                symbols,
                &source,
                entry,
                types,
                synthesized,
            )
        };
        let function = if frame.is_empty() {
            lower(types)
        } else {
            generics::with_frame(types, frame, lower)
        };
        types.classes.context = None;
        function
    }
}

/// Synthesize each dispatcher: test the receiver's class descriptor against
/// the classes sharing one implementation, then forward all physical slots.
pub(in crate::lower) fn dispatch_bodies(
    types: &ir::ValueTypes,
    functions: &mut Vec<ir::Function>,
) -> Result<()> {
    for (index, function) in dispatcher_functions(types)? {
        if functions.len() != index {
            return Err(CompilerError::Internal("class dispatcher order".into()));
        }
        functions.push(function);
    }
    Ok(())
}

/// V37 synthesizes the dispatchers after every body (and so every generic
/// class instance) exists, into the generated table at their slots.
pub(in crate::lower) fn late_dispatch_bodies(
    types: &ir::ValueTypes,
    generated: &mut BTreeMap<usize, ir::Function>,
) -> Result<()> {
    generated.extend(dispatcher_functions(types)?);
    Ok(())
}

fn dispatcher_functions(types: &ir::ValueTypes) -> Result<Vec<(usize, ir::Function)>> {
    let classes = &types.classes;
    let mut functions = Vec::new();
    for dispatcher in &classes.dispatchers {
        let root = &classes.list[dispatcher.root as usize];
        let base = &root.methods[&dispatcher.name];
        // Group descendants by the implementation they inherit.
        let mut implementations: BTreeMap<(u32, usize), Vec<u32>> = BTreeMap::new();
        for id in 0..classes.list.len() as u32 {
            if classes.descends(id, dispatcher.root) {
                let method = &classes.list[id as usize].methods[&dispatcher.name];
                implementations
                    .entry((method.owner, method.signature.index))
                    .or_default()
                    .push(classes.shape(id));
            }
        }
        if implementations
            .keys()
            .any(|(_, index)| *index == generic_classes::UNALLOCATED)
        {
            return Err(CompilerError::Internal(
                "unallocated dispatch target".into(),
            ));
        }
        functions.push((
            dispatcher.index,
            dispatcher_function(types, dispatcher.root, &base.signature, &implementations),
        ));
    }
    Ok(functions)
}

/// One dispatcher over `root`'s method with `signature`: test the receiver's
/// class descriptor against the classes sharing each overriding
/// implementation (grouped by owner and function index), then forward all
/// physical slots; the root's implementation is the fallback.
pub(super) fn dispatcher_function(
    types: &ir::ValueTypes,
    root: u32,
    signature: &Signature,
    implementations: &BTreeMap<(u32, usize), Vec<u32>>,
) -> ir::Function {
    let class = &types.classes.list[root as usize];
    let span = class.span;
    let params: Vec<Type> = std::iter::once(Type::Class(root))
        .chain(signature.params.iter().copied())
        .collect();
    let slot = params.len() as u32;
    let mut locals = params.clone();
    locals.push(Type::Number);
    let forward = |owner: u32, index: usize| {
        let args = params
            .iter()
            .enumerate()
            .map(|(position, ty)| Expr {
                span,
                ty: if position == 0 {
                    Type::Class(owner)
                } else {
                    *ty
                },
                kind: E::Local(position as u32),
            })
            .collect();
        let call = Expr {
            span,
            ty: signature.result,
            kind: E::Call(index, args),
        };
        if signature.result == Type::Void {
            vec![S::Expr(call), S::Return(None)]
        } else {
            vec![S::Return(Some(call))]
        }
    };
    let mut body = Vec::new();
    for ((owner, index), shapes) in implementations {
        if *owner == root {
            continue;
        }
        body.push(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsClass {
                    value: Box::new(Expr {
                        span,
                        ty: Type::Class(root),
                        kind: E::Local(0),
                    }),
                    shapes: shapes.clone(),
                    slot,
                },
            },
            forward(*owner, *index),
            Vec::new(),
        ));
    }
    body.extend(forward(root, signature.index));
    ir::Function {
        shape: signature.shape.clone(),
        self_slot: None,
        receiver: Some(Type::Class(root)),
        source: class.source.clone(),
        span,
        params,
        result: signature.result,
        locals,
        body,
        cells: Default::default(),
        captures: Vec::new(),
        environment: false,
        generator: None,
    }
}
