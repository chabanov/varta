//! Generic classes (v37 `class_generics`), implemented by monomorphization.
//! A generic class declaration is a template: it owns only its static
//! members (which cannot name the class's type parameters). Each distinct
//! type-argument list is an ordinary class of its own, with its own record
//! shape, descriptor, initializer and methods, laid out in the template's
//! module with the arguments bound, so `Page<number>` and `Page<string>` are
//! unrelated nominal classes like two hand-written ones.
//!
//! Instances are created from type references (`Page<number>`), `new`
//! expressions and `extends Base<T>` clauses, also while bodies are lowered.
//! Their member functions are therefore allocated lazily, after every
//! declared class and dispatcher ([`materialize`]), and lowered into the
//! generated table. Closed-world dispatch stays exact: dispatchers are
//! synthesized after the last body, and an instance never overrides an
//! inherited method (its overrides would have to join dispatchers decided
//! before it existed), so instance methods are always direct calls unless a
//! declared subclass of the instance overrides them.
use super::layout::{layout, static_method};
use super::*;
use generics::Missing;

/// An instance member whose body is not lowered yet.
pub(in crate::lower) struct PendingMember {
    pub(in crate::lower) index: usize,
    pub(in crate::lower) module: usize,
    class: u32,
    member: Member,
}

/// Placeholder function index of an instance member before [`materialize`].
pub(in crate::lower) const UNALLOCATED: usize = usize::MAX;

/// The instance of `template` for `args` (missing entries take defaults),
/// created and laid out on first use.
pub(in crate::lower) fn instantiate_class(
    template: u32,
    args: &[Option<Type>],
    missing: Missing,
    types: &mut ir::ValueTypes,
) -> Result<u32> {
    let class = &types.classes.list[template as usize];
    let params = Rc::clone(
        class
            .params
            .as_ref()
            .ok_or_else(|| CompilerError::Internal("generic class without parameters".into()))?,
    );
    let what = format!("class {}", class.name);
    let (module, source) = (class.module, class.source.clone());
    let caller = named::current(types);
    named::enter(types, module);
    let frame = generics::bind(&params, args, &what, missing, &source, types);
    named::enter(types, caller);
    let frame = frame?;
    let key: Vec<Type> = params
        .params
        .iter()
        .map(|p| frame[p.name.sym.as_ref()])
        .collect();
    if let Some(id) = types.classes.instances.get(&(template, key.clone())) {
        return Ok(*id);
    }
    if types.classes.list.len() >= MAX_RECORD_SHAPES {
        return Err(invalid("class count exceeds limit"));
    }
    let template_class = &types.classes.list[template as usize];
    let mut declaration = template_class.declaration.clone();
    declaration.type_params = None;
    let arguments: Vec<String> = key.iter().map(Type::to_string).collect();
    let id = types.classes.list.len() as u32;
    let instance = Class {
        name: format!("{}<{}>", template_class.name, arguments.join(", ")),
        module,
        source,
        span: template_class.span,
        declaration,
        parent: None,
        shape: 0,
        fields: Vec::new(),
        methods: BTreeMap::new(),
        statics: BTreeMap::new(),
        generic_methods: BTreeMap::new(),
        generic_statics: BTreeMap::new(),
        init: None,
        initializers: Vec::new(),
        state: State::Pending,
        prelude: template_class.prelude,
        opaque: template_class.opaque,
        params: None,
        template: Some(template),
        frame,
        unallocated: Vec::new(),
    };
    types.classes.list.push(instance);
    // Registered before layout, so `next: Node<T> | null` names itself.
    types.classes.instances.insert((template, key), id);
    if types.classes.ready {
        layout(id, types, None)?;
    } else {
        types.classes.deferred.push(id);
    }
    Ok(id)
}

/// The base class of `extends Name<args>` (or of a generic base whose
/// parameters all have defaults), resolved like the annotation `Name<args>`
/// in the class's module and type-argument frame.
pub(super) fn base(
    parent: u32,
    class: &ast::Class,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<u32> {
    let span = class.super_class.as_ref().map_or(class.span, |p| p.span());
    if !types.classes.get(parent).is_template() {
        return Err(source.locate(
            type_error(format!(
                "class {} is not generic and takes no type arguments",
                types.classes.get(parent).name
            )),
            span,
        ));
    }
    let args = match &class.super_type_params {
        Some(args) => generics::arguments(args, types)
            .map_err(|e| source.locate(e, args.span))?
            .into_iter()
            .map(Some)
            .collect(),
        None => Vec::new(),
    };
    instantiate_class(parent, &args, Missing::Reference, types).map_err(|e| source.locate(e, span))
}

/// A template owns the class's static members only; instance members are
/// laid out per instantiation. Statics may not name the type parameters
/// (TypeScript rule), so they are laid out without a frame.
pub(super) fn layout_template(
    id: u32,
    class: &ast::Class,
    source: &Source,
    types: &mut ir::ValueTypes,
    mut next: Option<&mut usize>,
) -> Result<()> {
    // Inherited statics come from the base declaration (`extends Base<T>`
    // inherits the statics of `Base`, which do not depend on `T`).
    let parent = match &class.super_class {
        Some(parent) => match &**parent {
            ast::Expr::Ident(name) => match named::class(types, name.sym.as_ref()) {
                Some(parent) => {
                    layout(parent, types, next.as_deref_mut())?;
                    Some(parent)
                }
                None => {
                    return Err(source.locate(
                        unsupported(&format!(
                            "class extends {}, which is not a declared class",
                            name.sym
                        )),
                        name.span,
                    ));
                }
            },
            _ => {
                return Err(source.locate(unsupported("class extends expression"), parent.span()));
            }
        },
        None => None,
    };
    let (mut statics, mut generic_statics) = match parent {
        Some(parent) => {
            let parent = types.classes.get(parent);
            (parent.statics.clone(), parent.generic_statics.clone())
        }
        None => Default::default(),
    };
    for member in &class.body {
        match member {
            ast::ClassMember::Method(method)
                if method.is_static && method.function.type_params.is_some() =>
            {
                let (name, entry) = generic_methods::declare(
                    id,
                    method,
                    &(BTreeMap::new(), generic_statics.clone()),
                    source,
                    types,
                )?;
                if statics.get(&name).is_some_and(|m| m.owner == id) {
                    return Err(source.locate(
                        invalid(format!("duplicate static class member: {name}")),
                        method.span,
                    ));
                }
                statics.remove(&name);
                generic_statics.insert(name, Rc::new(entry));
            }
            ast::ClassMember::Method(method) if method.is_static => {
                let (name, mut entry) = static_method(id, method, &statics, source, types)?;
                if generic_statics.get(&name).is_some_and(|m| m.owner == id) {
                    return Err(source.locate(
                        invalid(format!("duplicate static class member: {name}")),
                        method.span,
                    ));
                }
                generic_statics.remove(&name);
                let next = next.as_deref_mut().ok_or_else(|| {
                    CompilerError::Internal("template layout without slots".into())
                })?;
                entry.signature.index = types
                    .classes
                    .allocate(next, id, Member::Static(name.clone()))
                    .map_err(|e| source.locate(e, method.span))?;
                statics.insert(name, entry);
            }
            ast::ClassMember::ClassProp(property) if property.is_static => {
                return Err(source.locate(unsupported("static class field"), property.span));
            }
            _ => {}
        }
    }
    let entry = &mut types.classes.list[id as usize];
    entry.statics = statics;
    entry.generic_statics = generic_statics;
    Ok(())
}

/// Instances of generic classes are created after dispatch is fixed, so they
/// may not override inherited methods.
pub(super) fn override_rejected(
    id: u32,
    name: &str,
    source: &Source,
    span: Span,
    types: &ir::ValueTypes,
) -> CompilerError {
    let template = types.classes.get(id).template.unwrap_or(id);
    source.locate(
        unsupported(&format!(
            "override of {name} in generic class {}; generic class methods are not dispatched",
            types.classes.get(template).name
        )),
        span,
    )
}

/// Lay out the instances created before every declared class was laid out;
/// from now on instances lay out when they are created.
pub(super) fn lay_out_deferred(types: &mut ir::ValueTypes) -> Result<()> {
    types.classes.ready = true;
    for id in std::mem::take(&mut types.classes.deferred) {
        layout(id, types, None)?;
    }
    Ok(())
}

/// Allocate the member functions of every laid-out instance that has none
/// yet (bases first), queue their bodies and update the inherited entries of
/// every descendant.
pub(in crate::lower) fn materialize(types: &mut ir::ValueTypes, next: &mut usize) -> Result<()> {
    let pending: Vec<u32> = (0..types.classes.list.len() as u32)
        .filter(|id| !types.classes.list[*id as usize].unallocated.is_empty())
        .collect();
    for id in pending {
        materialize_class(id, types, next)?;
    }
    Ok(())
}

fn materialize_class(id: u32, types: &mut ir::ValueTypes, next: &mut usize) -> Result<()> {
    if types.classes.list[id as usize].unallocated.is_empty() {
        return Ok(());
    }
    if let Some(parent) = types.classes.list[id as usize].parent {
        materialize_class(parent, types, next)?;
    }
    let members = std::mem::take(&mut types.classes.list[id as usize].unallocated);
    for member in members {
        if *next >= MAX_FUNCTIONS {
            let class = &types.classes.list[id as usize];
            return Err(class.source.locate(
                invalid("too many functions including generic class instances"),
                class.span,
            ));
        }
        let index = *next;
        *next += 1;
        let class = &mut types.classes.list[id as usize];
        match &member {
            Member::Init => {
                if let Some(init) = class.init.as_mut() {
                    init.index = index;
                }
            }
            Member::Method(name) => {
                if let Some(method) = class.methods.get_mut(name) {
                    method.signature.index = index;
                }
            }
            Member::Static(_) => {
                return Err(CompilerError::Internal(
                    "static member of an instance".into(),
                ));
            }
        }
        let module = types.classes.list[id as usize].module;
        types.classes.pending_members.push_back(PendingMember {
            index,
            module,
            class: id,
            member,
        });
    }
    // Descendants copied the placeholder indices of inherited members.
    let own: Vec<(String, usize)> = types.classes.list[id as usize]
        .methods
        .iter()
        .filter(|(_, m)| m.owner == id)
        .map(|(name, m)| (name.clone(), m.signature.index))
        .collect();
    for other in 0..types.classes.list.len() as u32 {
        if other == id || !types.classes.descends(other, id) {
            continue;
        }
        for (name, index) in &own {
            if let Some(method) = types.classes.list[other as usize].methods.get_mut(name)
                && method.owner == id
            {
                method.signature.index = *index;
            }
        }
    }
    Ok(())
}

/// The next instance member whose body is pending.
pub(in crate::lower) fn next_member(types: &mut ir::ValueTypes) -> Option<PendingMember> {
    types.classes.pending_members.pop_front()
}

/// Lower an instance member body in the template's module.
pub(in crate::lower) fn member_body(
    pending: PendingMember,
    scope: (&link::Symbols, &module_constants::Scope),
    entry: usize,
    types: &mut ir::ValueTypes,
    synthesized: (
        &mut BTreeMap<usize, ir::Function>,
        &mut usize,
        &mut BTreeMap<usize, flow_calls::Impact>,
    ),
) -> Result<ir::Function> {
    let PendingMember {
        index,
        module,
        class,
        member,
    } = pending;
    named::enter(types, module);
    dispatch::member_function((index, class, &member), scope, entry, types, synthesized)
}

impl Lower<'_> {
    /// Allocate the member functions of instances created since the last
    /// call, before a member of a class is called.
    pub(in crate::lower) fn class_functions(&mut self) -> Result<()> {
        if self
            .types
            .classes
            .list
            .iter()
            .any(|c| !c.unallocated.is_empty())
        {
            materialize(self.types, self.next_function)?;
        }
        Ok(())
    }
    /// The instance a `new` of the generic class `template` constructs:
    /// explicit type arguments, the contextual class type, or arguments
    /// inferred from the constructor parameters. Returns the instance and the
    /// arguments lowered by the inference.
    pub(in crate::lower) fn generic_class_instance(
        &mut self,
        new: &ast::NewExpr,
        template: u32,
        expected: Option<Type>,
    ) -> Result<(u32, Option<ast::CallExpr>, Vec<Expr>)> {
        let span = new.span;
        if let Some(explicit) = &new.type_args {
            let args: Vec<Option<Type>> = generics::arguments(explicit, self.types)
                .map_err(|e| self.source.locate(e, explicit.span))?
                .into_iter()
                .map(Some)
                .collect();
            let id = instantiate_class(template, &args, Missing::Reference, self.types)
                .map_err(|e| self.source.locate(e, span))?;
            return Ok((id, None, Vec::new()));
        }
        let contextual = expected.and_then(|ty| {
            unions::members(ty, self.types)
                .into_iter()
                .find_map(|member| match member {
                    Type::Class(id) if self.types.classes.get(id).template == Some(template) => {
                        Some(id)
                    }
                    _ => None,
                })
        });
        if let Some(id) = contextual {
            return Ok((id, None, Vec::new()));
        }
        let class = self.types.classes.get(template);
        let params = class
            .params
            .as_ref()
            .map(|p| (**p).clone())
            .ok_or_else(|| CompilerError::Internal("generic class without parameters".into()))?;
        let constructor = class
            .declaration
            .body
            .iter()
            .find_map(|member| match member {
                ast::ClassMember::Constructor(ctor) => Some(ctor.clone()),
                _ => None,
            });
        let name = class.name.clone();
        let (module, source) = (class.module, class.source.clone());
        let Some(constructor) = constructor else {
            return Err(self.source.locate(
                type_error(format!(
                    "cannot infer the type arguments of class {name} without its own constructor; \
                     provide explicit type arguments"
                )),
                span,
            ));
        };
        let function = constructor_function(&constructor, &source)?;
        let function = generics::Function {
            module,
            kind: "class",
            name,
            declaration: function_declaration(CONSTRUCTOR_NAME, constructor.span, function),
            params,
            source,
        };
        let call = ast::CallExpr {
            span,
            ctxt: Default::default(),
            callee: ast::Callee::Expr(new.callee.clone()),
            args: new.args.clone().unwrap_or_default(),
            type_args: None,
        };
        let (args, values) = self.infer(&function, &call, None)?;
        let id = instantiate_class(template, &args, Missing::Inference, self.types)
            .map_err(|e| self.source.locate(e, span))?;
        Ok((id, Some(call), values))
    }
    /// The instance of `template` that a value of type `ty` may hold, for
    /// `value instanceof Template`: a union holds at most one class member.
    pub(in crate::lower) fn template_instance(&self, template: u32, ty: Type) -> Option<u32> {
        unions::members(ty, self.types)
            .into_iter()
            .find_map(|member| match member {
                Type::Class(id) => {
                    let mut class = Some(id);
                    while let Some(current) = class {
                        if self.types.classes.get(current).template == Some(template) {
                            return Some(current);
                        }
                        class = self.types.classes.get(current).parent;
                    }
                    None
                }
                _ => None,
            })
    }
}
