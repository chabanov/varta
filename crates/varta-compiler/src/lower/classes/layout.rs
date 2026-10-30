//! Class layout: inherited-prefix record shapes, member tables, constructor
//! signatures and function indices, assigned parents first.
use super::*;

/// Lay out the classes declared in `module` (parents first) and assign their
/// initializer and method function indices after the module's functions.
pub(in crate::lower) fn layout_module(
    module: usize,
    symbols: &BTreeMap<String, Signature>,
    types: &mut ir::ValueTypes,
    next: &mut usize,
) -> Result<()> {
    // Generic class instances (v37) are laid out when they are created.
    let ids: Vec<u32> = (0..types.classes.list.len() as u32)
        .filter(|id| {
            let class = &types.classes.list[*id as usize];
            class.module == module && class.template.is_none()
        })
        .collect();
    for id in ids {
        let class = &types.classes.list[id as usize];
        if symbols.contains_key(&class.name) {
            return Err(class.source.locate(
                invalid(format!("duplicate class and function name: {}", class.name)),
                class.span,
            ));
        }
        layout(id, types, Some(&mut *next))?;
    }
    Ok(())
}

/// Lay out one class. `next` allocates the member functions of declared
/// classes; it is `None` only once every declared class is laid out (generic
/// class instances never allocate here, see [`generic_classes::materialize`]).
pub(super) fn layout(id: u32, types: &mut ir::ValueTypes, next: Option<&mut usize>) -> Result<()> {
    let class = &types.classes.list[id as usize];
    match class.state {
        State::Ready => return Ok(()),
        State::Laying => {
            return Err(class
                .source
                .locate(unsupported("recursive class inheritance"), class.span));
        }
        State::Pending => {}
    }
    let source = class.source.clone();
    let declaration = class.declaration.clone();
    let module = class.module;
    types.classes.list[id as usize].state = State::Laying;
    let caller = named::current(types);
    named::enter(types, module);
    let frame = types.classes.list[id as usize].frame.clone();
    let result = if types.classes.list[id as usize].template.is_some() {
        generics::with_frame(types, frame, |types| {
            layout_members(id, &declaration, &source, types, next)
        })
    } else {
        layout_members(id, &declaration, &source, types, next)
    };
    named::enter(types, caller);
    result?;
    types.classes.list[id as usize].state = State::Ready;
    Ok(())
}

/// The function index of a member: allocated now for a declared class, and
/// left for [`generic_classes::materialize`] for a generic class instance.
fn allocate(
    types: &mut ir::ValueTypes,
    next: Option<&mut usize>,
    id: u32,
    member: Member,
) -> Result<usize> {
    if types.classes.list[id as usize].template.is_some() {
        types.classes.list[id as usize].unallocated.push(member);
        return Ok(generic_classes::UNALLOCATED);
    }
    let next = next.ok_or_else(|| CompilerError::Internal("class layout without slots".into()))?;
    let limit = types.function_limit();
    types.classes.allocate((next, limit), id, member)
}

/// A subclass may not redeclare an inherited private name: in JavaScript the
/// two would be distinct members, which one shared table cannot represent.
fn private_shadow(name: &str, source: &Source, span: Span) -> CompilerError {
    source.locate(
        unsupported(&format!("#private name {name} redeclared by a subclass")),
        span,
    )
}

fn check_class_header(class: &ast::Class, types: &ir::ValueTypes) -> Result<()> {
    if !class.decorators.is_empty() {
        return Err(unsupported("class decorator"));
    }
    // Generic classes and generic bases (v37) are instantiated per argument list.
    if !types.profile.class_generics()
        && (class.type_params.is_some() || class.super_type_params.is_some())
    {
        return Err(unsupported("generic class"));
    }
    if class.is_abstract {
        return Err(unsupported("abstract class"));
    }
    // `implements` is a checked conformance (v37 `class_generics`).
    if !class.implements.is_empty() && !types.profile.class_generics() {
        return Err(unsupported("class implements clause"));
    }
    Ok(())
}

fn parent_class(
    class: &ast::Class,
    source: &Source,
    types: &mut ir::ValueTypes,
    next: Option<&mut usize>,
) -> Result<Option<u32>> {
    let Some(parent) = &class.super_class else {
        return Ok(None);
    };
    let ast::Expr::Ident(name) = &**parent else {
        return Err(source.locate(unsupported("class extends expression"), parent.span()));
    };
    if let Some(parent) = named::class(types, name.sym.as_ref()) {
        // `extends Base<A>` or a generic base with defaults (v37).
        let parent = if types.classes.get(parent).is_template() || class.super_type_params.is_some()
        {
            generic_classes::base(parent, class, source, types)?
        } else {
            parent
        };
        layout(parent, types, next)?;
        return Ok(Some(parent));
    }
    let family = varta_common::value::ErrorClass::from_name(name.sym.as_ref());
    if let Some(family) = family.filter(|_| types.profile.error_subclasses()) {
        return error_classes::root(family, source, name.span, types).map(Some);
    }
    let kind = if family.is_some() {
        "class extends a built-in Error class".to_owned()
    } else {
        format!("class extends {}, which is not a declared class", name.sym)
    };
    Err(source.locate(unsupported(&kind), name.span))
}

fn layout_members(
    id: u32,
    class: &ast::Class,
    source: &Source,
    types: &mut ir::ValueTypes,
    mut next: Option<&mut usize>,
) -> Result<()> {
    check_class_header(class, types).map_err(|e| source.locate(e, class.span))?;
    if types.classes.list[id as usize].is_template() {
        return generic_classes::layout_template(id, class, source, types, next);
    }
    let instance = types.classes.list[id as usize].template.is_some();
    let parent = parent_class(class, source, types, next.as_deref_mut())?;
    let (mut fields, mut methods, mut statics) = match parent {
        Some(parent) => {
            let parent = &types.classes.list[parent as usize];
            (
                parent.fields.clone(),
                parent.methods.clone(),
                parent.statics.clone(),
            )
        }
        None => (Vec::new(), BTreeMap::new(), BTreeMap::new()),
    };
    let mut generic = match parent {
        Some(parent) => {
            let parent = &types.classes.list[parent as usize];
            (
                parent.generic_methods.clone(),
                parent.generic_statics.clone(),
            )
        }
        None => Default::default(),
    };
    let mut initializers = Vec::new();
    let mut constructor = None;
    for member in &class.body {
        match member {
            ast::ClassMember::ClassProp(property) => {
                let (field, initializer) =
                    field(id, property, types).map_err(|e| source.locate(e, property.span))?;
                if is_private_name(&field.name) && fields.iter().any(|f| f.name == field.name) {
                    return Err(private_shadow(&field.name, source, property.span));
                }
                if types.profile.mixed_index_records()
                    && let Some(inherited) = fields
                        .iter_mut()
                        .find(|f| f.name == field.name && f.owner != id)
                {
                    // A compatible redeclaration of an inherited field (v35):
                    // the slot is shared, and unless the declaration is
                    // ambient it is defined again after `super(...)` returns.
                    redeclare(inherited, &field).map_err(|e| source.locate(e, property.span))?;
                    if !property.declare {
                        initializers.push((
                            field.name.clone(),
                            initializer_or_undefined(initializer, property),
                        ));
                    }
                    continue;
                }
                if property.declare {
                    return Err(source.locate(unsupported("ambient class field"), property.span));
                }
                if fields.iter().any(|f| f.name == field.name)
                    || methods.contains_key(&field.name)
                    || generic.0.contains_key(&field.name)
                {
                    return Err(source.locate(
                        invalid(format!("duplicate class member: {}", field.name)),
                        property.span,
                    ));
                }
                if let Some(value) = initializer {
                    initializers.push((field.name.clone(), value));
                } else if property.is_optional
                    // V37 (`language_core`): an uninitialized field whose type
                    // admits `undefined` starts as `undefined`, as in TypeScript.
                    || types.profile.language_core()
                        && unions::members(field.ty, types).contains(&Type::Undefined)
                {
                    // `x?: T` defines the own property `x` as `undefined`.
                    initializers
                        .push((field.name.clone(), initializer_or_undefined(None, property)));
                }
                fields.push(field);
            }
            // Generic methods (v37) are instantiated per type-argument list.
            ast::ClassMember::Method(method)
                if method.function.type_params.is_some() && types.profile.class_generics() =>
            {
                if instance
                    && !method.is_static
                    && generic
                        .0
                        .contains_key(&member_name(&method.key).unwrap_or_default())
                {
                    let name = member_name(&method.key).unwrap_or_default();
                    return Err(generic_classes::override_rejected(
                        id,
                        &name,
                        source,
                        method.span,
                        types,
                    ));
                }
                if instance && method.is_static {
                    continue;
                }
                let (name, entry) = generic_methods::declare(id, method, &generic, source, types)?;
                let owner = if entry.is_static {
                    statics.get(&name).map(|m| m.owner)
                } else {
                    fields
                        .iter()
                        .find(|f| f.name == name)
                        .map(|f| f.owner)
                        .or_else(|| methods.get(&name).map(|m| m.owner))
                };
                match owner {
                    Some(owner) if owner == id => {
                        return Err(source.locate(
                            invalid(format!("duplicate class member: {name}")),
                            method.span,
                        ));
                    }
                    Some(_) => {
                        return Err(source.locate(
                            type_error(format!(
                                "override of {name} must keep the inherited method signature"
                            )),
                            method.key.span(),
                        ));
                    }
                    None => {}
                }
                if entry.is_static {
                    generic.1.insert(name, Rc::new(entry));
                } else {
                    types.classes.method_names.insert(name.clone());
                    generic.0.insert(name, Rc::new(entry));
                }
            }
            // Static members belong to the template of a generic class (v37).
            ast::ClassMember::Method(method) if method.is_static && instance => {}
            ast::ClassMember::Method(method) if method.is_static => {
                let (name, mut entry) = static_method(id, method, &statics, source, types)?;
                if let Some(other) = generic.1.get(&name) {
                    return Err(generic_methods::collision(id, other, &name, method, source));
                }
                entry.signature.index =
                    allocate(types, next.as_deref_mut(), id, Member::Static(name.clone()))
                        .map_err(|e| source.locate(e, method.span))?;
                statics.insert(name, entry);
            }
            ast::ClassMember::Method(method) => {
                if let Ok(name) = member_name(&method.key)
                    && is_private_name(&name)
                    && methods.get(&name).is_some_and(|m| m.owner != id)
                {
                    return Err(private_shadow(&name, source, method.span));
                }
                let (name, entry) = self::method(id, method, &methods, source, types)?;
                if let Some(other) = generic.0.get(&name) {
                    return Err(generic_methods::collision(id, other, &name, method, source));
                }
                if fields.iter().any(|f| f.name == name) {
                    return Err(source.locate(
                        invalid(format!("duplicate class member: {name}")),
                        method.span,
                    ));
                }
                if instance && methods.contains_key(&name) {
                    return Err(generic_classes::override_rejected(
                        id,
                        &name,
                        source,
                        method.span,
                        types,
                    ));
                }
                let mut entry = entry;
                entry.signature.index =
                    allocate(types, next.as_deref_mut(), id, Member::Method(name.clone()))
                        .map_err(|e| source.locate(e, method.span))?;
                types.classes.method_names.insert(name.clone());
                methods.insert(name, entry);
            }
            ast::ClassMember::Constructor(ctor) => {
                if constructor.replace(ctor).is_some() || ctor.body.is_none() {
                    return Err(source.locate(
                        unsupported("constructor overload or missing constructor body"),
                        ctor.span,
                    ));
                }
            }
            ast::ClassMember::Empty(_) => {}
            ast::ClassMember::PrivateProp(p) => {
                return Err(source.locate(unsupported("#private class field"), p.span));
            }
            ast::ClassMember::PrivateMethod(p) => {
                return Err(source.locate(unsupported("#private class method"), p.span));
            }
            ast::ClassMember::TsIndexSignature(s) => {
                return Err(source.locate(unsupported("class index signature"), s.span));
            }
            ast::ClassMember::StaticBlock(s) => {
                return Err(source.locate(unsupported("static class block"), s.span));
            }
            ast::ClassMember::AutoAccessor(a) => {
                return Err(source.locate(unsupported("class auto-accessor"), a.span));
            }
        }
    }
    if fields.len() > types.max_record_fields() {
        return Err(source.locate(invalid("class field count exceeds limit"), class.span));
    }
    let init = initializer(id, constructor, parent, source, types)?;
    let mut init = init;
    init.index =
        allocate(types, next, id, Member::Init).map_err(|e| source.locate(e, class.span))?;
    if types.records.len() >= types.profile.limits().record_shapes {
        return Err(source.locate(invalid("object shape count exceeds limit"), class.span));
    }
    let shape = types.records.len() as u32;
    types.records.push(ir::RecordShape {
        fields: fields
            .iter()
            .map(|field| ir::RecordField {
                name: field.name.clone(),
                ty: field.ty,
                optional: false,
            })
            .collect(),
        class: Some(id),
    });
    let entry = &mut types.classes.list[id as usize];
    entry.parent = parent;
    entry.shape = shape;
    entry.fields = fields;
    entry.methods = methods;
    entry.statics = statics;
    (entry.generic_methods, entry.generic_statics) = generic;
    entry.init = Some(init);
    entry.initializers = initializers;
    implements::check(id, class, source, types)
}

fn field(
    owner: u32,
    property: &ast::ClassProp,
    types: &mut ir::ValueTypes,
) -> Result<(Field, Option<Box<ast::Expr>>)> {
    let unsupported_kind = if property.is_static {
        Some("static class field")
    } else if !property.decorators.is_empty() {
        Some("class field decorator")
    } else if property.is_abstract {
        Some("abstract class field")
    } else if property.is_optional && !types.profile.mixed_index_records() {
        Some("optional class field")
    } else if property.declare && !types.profile.mixed_index_records() {
        Some("ambient class field")
    } else if property.declare && property.value.is_some() {
        Some("ambient class field initializer")
    } else if property.definite {
        Some("definite assignment assertion on a class field")
    } else if property.is_override {
        Some("overriding class field")
    } else {
        None
    };
    if let Some(kind) = unsupported_kind {
        return Err(unsupported(kind));
    }
    let name = member_name(&property.key)?;
    let ty = match annotation(property.type_ann.as_deref(), types)? {
        Some(ty) => ty,
        None if property.readonly
            && types.profile.language_core()
            && let Some(literal) = property.value.as_deref().and_then(readonly_literal) =>
        {
            literals::annotation(&literal, types)?
        }
        None => property
            .value
            .as_deref()
            .and_then(|value| parameters::literal(value, types))
            .map(Ok)
            .or_else(|| {
                property
                    .value
                    .as_deref()
                    .and_then(|value| constructed_type(value, types).transpose())
            })
            .transpose()?
            .ok_or_else(|| {
                invalid(format!(
                    "class field {name} requires a type annotation or a literal initializer"
                ))
            })?,
    };
    // An optional field (v35) holds `undefined` when not assigned.
    let ty = if property.is_optional {
        unions::intern(vec![ty, Type::Undefined], types)?
    } else {
        ty
    };
    if matches!(ty, Type::Function(_)) && !types.profile.function_fields() {
        return Err(unsupported("function-valued class field"));
    }
    if ty.profile_kind(types.profile).is_none()
        || ty == Type::BigInt && !types.profile.bigint_values()
    {
        return Err(unsupported("class field value type"));
    }
    Ok((
        Field {
            name,
            ty,
            owner,
            access: Access::from_ast(property.accessibility),
            readonly: property.readonly,
            redeclared: None,
        },
        property.value.clone(),
    ))
}

/// The literal type of a readonly field's literal initializer (v37
/// `language_core`): TypeScript keeps `readonly kind = 'leaf'` at the type
/// `'leaf'`, so such fields discriminate unions of classes.
fn readonly_literal(value: &ast::Expr) -> Option<ast::TsLitType> {
    let lit = match stmt::unparen(value) {
        ast::Expr::Lit(ast::Lit::Str(text)) => ast::TsLit::Str(text.clone()),
        ast::Expr::Lit(ast::Lit::Num(number)) => ast::TsLit::Number(number.clone()),
        ast::Expr::Lit(ast::Lit::Bool(value)) => ast::TsLit::Bool(*value),
        _ => return None,
    };
    Some(ast::TsLitType {
        span: value.span(),
        lit,
    })
}

/// The class type of a `new C(...)` (or `new C<A>(...)`) field initializer
/// (v37 `class_generics`, as in the pilot's `#controller = new
/// AbortController()`); a generic class needs explicit type arguments here.
fn constructed_type(value: &ast::Expr, types: &mut ir::ValueTypes) -> Result<Option<Type>> {
    if !types.profile.class_generics() {
        return Ok(None);
    }
    let ast::Expr::New(new) = stmt::unparen(value) else {
        return Ok(None);
    };
    let ast::Expr::Ident(name) = &*new.callee else {
        return Ok(None);
    };
    // `new Map<K, V>()`, `new Set<T>()` and `new Uint8Array(n)` (v37
    // `collections`); without type arguments TypeScript infers `any`.
    if let Some(kind) = collections::builtin(name.sym.as_ref(), types) {
        let args: Vec<&ast::TsType> = new
            .type_args
            .iter()
            .flat_map(|args| args.params.iter().map(|ty| &**ty))
            .collect();
        return Ok(match (kind, args.as_slice()) {
            (ir::CollectionKind::Map, [key, value]) => {
                let key = keyword_type(key, types)?;
                let value = keyword_type(value, types)?;
                Some(collections::intern(kind, key, value, types)?)
            }
            (ir::CollectionKind::Set, [value]) => {
                let value = keyword_type(value, types)?;
                Some(collections::intern(kind, value, value, types)?)
            }
            (ir::CollectionKind::Uint8Array, []) => Some(collections::bytes(types)?),
            _ => None,
        });
    }
    let Some(class) = named::class(types, name.sym.as_ref()) else {
        return Ok(None);
    };
    if !types.classes.get(class).is_template() {
        return Ok(Some(Type::Class(class)));
    }
    let Some(args) = &new.type_args else {
        return Ok(None);
    };
    let args: Vec<Option<Type>> = generics::arguments(args, types)?
        .into_iter()
        .map(Some)
        .collect();
    generic_classes::instantiate_class(class, &args, generics::Missing::Reference, types)
        .map(|id| Some(Type::Class(id)))
}

/// Check a subclass redeclaration of an inherited field (v35): TypeScript
/// requires an assignable type, and Varta the same type, because the slot is
/// shared with the base class, whose own writes would otherwise break a
/// narrower type. A private base field cannot be redeclared, the access level
/// may not become stricter, and the redeclaration may add `readonly`.
fn redeclare(inherited: &mut Field, own: &Field) -> Result<()> {
    if inherited.access == Access::Private {
        return Err(type_error(format!(
            "class field {} redeclares a private inherited field",
            own.name
        )));
    }
    let stricter = matches!(
        (inherited.access, own.access),
        (Access::Public, Access::Protected | Access::Private)
            | (Access::Protected, Access::Private)
    );
    if stricter {
        return Err(type_error(format!(
            "class field {} must keep the inherited accessibility",
            own.name
        )));
    }
    if inherited.readonly && !own.readonly {
        return Err(type_error(format!(
            "class field {} must stay readonly like the inherited field",
            own.name
        )));
    }
    if inherited.ty != own.ty {
        return Err(type_error(format!(
            "class field {} redeclares the inherited field with type {} instead of {};              a redeclaration must keep the inherited type",
            own.name, own.ty, inherited.ty
        )));
    }
    inherited.access = own.access;
    inherited.readonly = own.readonly;
    inherited.redeclared = Some(own.owner);
    Ok(())
}

/// The initializer of a field definition: its own, or `undefined`.
fn initializer_or_undefined(
    initializer: Option<Box<ast::Expr>>,
    property: &ast::ClassProp,
) -> Box<ast::Expr> {
    initializer.unwrap_or_else(|| {
        Box::new(ast::Expr::Ident(ast::Ident::new(
            "undefined".into(),
            property.span,
            Default::default(),
        )))
    })
}

fn method(
    owner: u32,
    method: &ast::ClassMethod,
    inherited: &BTreeMap<String, Method>,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<(String, Method)> {
    let unsupported_kind = match method.kind {
        ast::MethodKind::Getter if method.is_abstract => Some("abstract class getter"),
        ast::MethodKind::Getter if !method.function.params.is_empty() => {
            Some("class getter parameters")
        }
        ast::MethodKind::Getter => None,
        ast::MethodKind::Setter => Some("class setter"),
        ast::MethodKind::Method if method.is_abstract => Some("abstract class method"),
        ast::MethodKind::Method if method.is_optional => Some("optional class method"),
        ast::MethodKind::Method if method.function.this_param.is_some() => {
            Some("explicit this parameter on a class method")
        }
        // Async instance methods are split like async functions (v35).
        ast::MethodKind::Method if method.function.is_async && !types.profile.async_methods() => {
            Some("async class method")
        }
        ast::MethodKind::Method => None,
    };
    if let Some(kind) = unsupported_kind {
        return Err(source.locate(unsupported(kind), method.span));
    }
    let prelude = crate::modules::engine::is_prelude_source(source);
    let name = method_name(&method.key, types.profile, prelude)
        .map_err(|e| source.locate(e, method.key.span()))?;
    if name == "toJSON" {
        return Err(source.locate(
            unsupported("class toJSON method; JSON serialization uses own fields"),
            method.key.span(),
        ));
    }
    let declaration = function_declaration(&name, method.key.span(), method.function.clone());
    let mut signature = signature::check(&declaration, 0, source, types)
        .map_err(|e| source.locate(e, method.key.span()))?;
    signature.receiver = Some(Type::Class(owner));
    receiver_width(&signature).map_err(|e| source.locate(e, method.key.span()))?;
    let access = Access::from_ast(method.accessibility);
    let getter = method.kind == ast::MethodKind::Getter;
    if getter && signature.result == Type::Void {
        return Err(source.locate(
            type_error(format!("class getter {name} must return a value")),
            method.key.span(),
        ));
    }
    let root = match inherited.get(&name) {
        Some(base) if base.owner == owner => {
            return Err(source.locate(
                invalid(format!("duplicate class member: {name}")),
                method.span,
            ));
        }
        Some(base) => {
            if base.access == Access::Private || base.access != access {
                return Err(source.locate(
                    type_error(format!(
                        "override of {name} must keep the inherited non-private accessibility"
                    )),
                    method.key.span(),
                ));
            }
            if base.getter != getter {
                return Err(source.locate(
                    type_error(format!(
                        "override of {name} must keep the inherited member kind"
                    )),
                    method.key.span(),
                ));
            }
            let expected = &base.signature;
            if expected.params != signature.params
                || expected.shape != signature.shape
                || expected.result != signature.result
            {
                return Err(source.locate(
                    type_error(format!(
                        "override of {name} must keep the inherited method signature"
                    )),
                    method.key.span(),
                ));
            }
            base.root
        }
        None => owner,
    };
    Ok((
        name,
        Method {
            owner,
            root,
            signature,
            access,
            dispatch: None,
            getter,
        },
    ))
}

/// A static method: a receiver-free function named through its class. A
/// subclass may redeclare an inherited static; calls resolve statically.
pub(super) fn static_method(
    owner: u32,
    method: &ast::ClassMethod,
    inherited: &BTreeMap<String, Method>,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<(String, Method)> {
    let unsupported_kind = match method.kind {
        ast::MethodKind::Getter | ast::MethodKind::Setter => Some("static class accessor"),
        ast::MethodKind::Method if method.is_abstract => Some("abstract class method"),
        ast::MethodKind::Method if method.is_optional => Some("optional class method"),
        ast::MethodKind::Method if method.function.this_param.is_some() => {
            Some("explicit this parameter on a class method")
        }
        ast::MethodKind::Method => None,
    };
    if let Some(kind) = unsupported_kind {
        return Err(source.locate(unsupported(kind), method.span));
    }
    let name = member_name(&method.key).map_err(|e| source.locate(e, method.key.span()))?;
    if inherited.get(&name).is_some_and(|m| m.owner == owner) {
        return Err(source.locate(
            invalid(format!("duplicate static class member: {name}")),
            method.span,
        ));
    }
    let declaration = function_declaration(&name, method.key.span(), method.function.clone());
    let signature = signature::check(&declaration, 0, source, types)
        .map_err(|e| source.locate(e, method.key.span()))?;
    Ok((
        name,
        Method {
            owner,
            root: owner,
            signature,
            access: Access::from_ast(method.accessibility),
            dispatch: None,
            getter: false,
        },
    ))
}

/// Constructor initializer signature: `(this: C, ...params) => void`.
fn initializer(
    owner: u32,
    constructor: Option<&ast::Constructor>,
    parent: Option<u32>,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<Signature> {
    let Some(ctor) = constructor else {
        let inherited = parent.and_then(|p| types.classes.list[p as usize].init.clone());
        let mut signature = inherited.unwrap_or(Signature {
            shape: ir::ParameterShape::fixed(0),
            receiver: None,
            index: 0,
            names: Vec::new(),
            params: Vec::new(),
            result: Type::Void,
        });
        if signature.shape.rest {
            return Err(source.locate(
                unsupported("implicit derived constructor of a rest-parameter base"),
                types.classes.list[owner as usize].span,
            ));
        }
        signature.receiver = Some(Type::Class(owner));
        return Ok(signature);
    };
    if ctor
        .accessibility
        .is_some_and(|a| a != ast::Accessibility::Public)
    {
        return Err(source.locate(unsupported("private or protected constructor"), ctor.span));
    }
    let function = constructor_function(ctor, source)?;
    let declaration = function_declaration(CONSTRUCTOR_NAME, ctor.span, function);
    let mut signature = signature::check(&declaration, 0, source, types)
        .map_err(|e| source.locate(e, ctor.span))?;
    signature.receiver = Some(Type::Class(owner));
    signature.result = Type::Void;
    receiver_width(&signature).map_err(|e| source.locate(e, ctor.span))?;
    Ok(signature)
}
