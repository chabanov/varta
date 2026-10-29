//! Class function bodies: constructor initializers and methods are lowered
//! like declarations with a class receiver; dispatchers are synthesized IR.
use super::*;

/// After every module is laid out, give each overridden root method one
/// dispatcher shared by the root class and all of its descendants.
pub(in crate::lower) fn dispatchers(types: &mut ir::ValueTypes, next: &mut usize) -> Result<()> {
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
        if *next >= MAX_FUNCTIONS {
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
    Ok(())
}

/// Lower the initializer and method bodies of the classes of `module` in the
/// function-index order assigned by `layout_module`.
pub(in crate::lower) fn bodies(
    module: usize,
    symbols: &link::Symbols,
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
        let (declaration, signature, init) = match &member {
            Member::Init => {
                let declaration = constructors::declaration(class, types)?;
                let signature = types.classes.list[class as usize]
                    .init
                    .clone()
                    .ok_or_else(|| CompilerError::Internal("missing class initializer".into()))?;
                (declaration, signature, Some(index))
            }
            Member::Method(name) => {
                let info = &types.classes.list[class as usize];
                let method = info
                    .declaration
                    .body
                    .iter()
                    .find_map(|member| match member {
                        ast::ClassMember::Method(m)
                            if member_name(&m.key).is_ok_and(|n| &n == name) =>
                        {
                            Some(m)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| CompilerError::Internal("missing class method".into()))?;
                let declaration =
                    function_declaration(name, method.key.span(), method.function.clone());
                (declaration, info.methods[name].signature.clone(), None)
            }
        };
        let source = types.classes.list[class as usize].source.clone();
        types.classes.context = Some(Context { class, init });
        let function = link::body(
            &declaration,
            &signature,
            symbols,
            &source,
            entry,
            types,
            (&mut *generated, &mut *next, &mut *effects),
        );
        types.classes.context = None;
        functions.push(function?);
    }
    Ok(())
}

/// Synthesize each dispatcher: test the receiver's class descriptor against
/// the classes sharing one implementation, then forward all physical slots.
pub(in crate::lower) fn dispatch_bodies(
    types: &ir::ValueTypes,
    functions: &mut Vec<ir::Function>,
) -> Result<()> {
    let classes = &types.classes;
    for dispatcher in &classes.dispatchers {
        if functions.len() != dispatcher.index {
            return Err(CompilerError::Internal("class dispatcher order".into()));
        }
        let root = &classes.list[dispatcher.root as usize];
        let base = &root.methods[&dispatcher.name];
        let signature = &base.signature;
        let span = root.span;
        let params: Vec<Type> = std::iter::once(Type::Class(dispatcher.root))
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
        let mut body = Vec::new();
        for ((owner, index), shapes) in &implementations {
            if *owner == dispatcher.root {
                continue;
            }
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::IsClass {
                        value: Box::new(Expr {
                            span,
                            ty: Type::Class(dispatcher.root),
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
        body.extend(forward(dispatcher.root, signature.index));
        functions.push(ir::Function {
            shape: signature.shape.clone(),
            self_slot: None,
            receiver: Some(Type::Class(dispatcher.root)),
            source: root.source.clone(),
            span,
            params,
            result: signature.result,
            locals,
            body,
            cells: Default::default(),
            captures: Vec::new(),
            environment: false,
        });
    }
    Ok(())
}
