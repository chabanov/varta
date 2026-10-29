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
    let ids: Vec<u32> = (0..types.classes.list.len() as u32)
        .filter(|id| types.classes.list[*id as usize].module == module)
        .collect();
    for id in ids {
        let class = &types.classes.list[id as usize];
        if symbols.contains_key(&class.name) {
            return Err(class.source.locate(
                invalid(format!("duplicate class and function name: {}", class.name)),
                class.span,
            ));
        }
        layout(id, types, next)?;
    }
    Ok(())
}

fn layout(id: u32, types: &mut ir::ValueTypes, next: &mut usize) -> Result<()> {
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
    let result = layout_members(id, &declaration, &source, types, next);
    named::enter(types, caller);
    result?;
    types.classes.list[id as usize].state = State::Ready;
    Ok(())
}

fn check_class_header(class: &ast::Class) -> Result<()> {
    if !class.decorators.is_empty() {
        return Err(unsupported("class decorator"));
    }
    if class.type_params.is_some() || class.super_type_params.is_some() {
        return Err(unsupported("generic class"));
    }
    if class.is_abstract {
        return Err(unsupported("abstract class"));
    }
    if !class.implements.is_empty() {
        return Err(unsupported("class implements clause"));
    }
    Ok(())
}

fn parent_class(
    class: &ast::Class,
    source: &Source,
    types: &mut ir::ValueTypes,
    next: &mut usize,
) -> Result<Option<u32>> {
    let Some(parent) = &class.super_class else {
        return Ok(None);
    };
    let ast::Expr::Ident(name) = &**parent else {
        return Err(source.locate(unsupported("class extends expression"), parent.span()));
    };
    if let Some(parent) = named::class(types, name.sym.as_ref()) {
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
    next: &mut usize,
) -> Result<()> {
    check_class_header(class).map_err(|e| source.locate(e, class.span))?;
    let parent = parent_class(class, source, types, next)?;
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
    let mut initializers = Vec::new();
    let mut constructor = None;
    for member in &class.body {
        match member {
            ast::ClassMember::ClassProp(property) => {
                let (field, initializer) =
                    field(id, property, types).map_err(|e| source.locate(e, property.span))?;
                if fields.iter().any(|f| f.name == field.name) || methods.contains_key(&field.name)
                {
                    return Err(source.locate(
                        invalid(format!("duplicate class member: {}", field.name)),
                        property.span,
                    ));
                }
                if let Some(value) = initializer {
                    initializers.push((field.name.clone(), value));
                }
                fields.push(field);
            }
            ast::ClassMember::Method(method) if method.is_static => {
                let (name, mut entry) = static_method(id, method, &statics, source, types)?;
                entry.signature.index = types
                    .classes
                    .allocate(next, id, Member::Static(name.clone()))
                    .map_err(|e| source.locate(e, method.span))?;
                statics.insert(name, entry);
            }
            ast::ClassMember::Method(method) => {
                let (name, entry) = self::method(id, method, &methods, source, types)?;
                if fields.iter().any(|f| f.name == name) {
                    return Err(source.locate(
                        invalid(format!("duplicate class member: {name}")),
                        method.span,
                    ));
                }
                let mut entry = entry;
                entry.signature.index = types
                    .classes
                    .allocate(next, id, Member::Method(name.clone()))
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
    if fields.len() > MAX_RECORD_FIELDS {
        return Err(source.locate(invalid("class field count exceeds limit"), class.span));
    }
    let init = initializer(id, constructor, parent, source, types)?;
    let mut init = init;
    init.index = types
        .classes
        .allocate(next, id, Member::Init)
        .map_err(|e| source.locate(e, class.span))?;
    if types.records.len() >= MAX_RECORD_SHAPES {
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
    entry.init = Some(init);
    entry.initializers = initializers;
    Ok(())
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
    } else if property.is_optional {
        Some("optional class field")
    } else if property.declare {
        Some("ambient class field")
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
        None => property
            .value
            .as_deref()
            .and_then(|value| parameters::literal(value, types))
            .ok_or_else(|| {
                invalid(format!(
                    "class field {name} requires a type annotation or a literal initializer"
                ))
            })?,
    };
    if matches!(ty, Type::Function(_)) {
        return Err(unsupported("function-valued class field"));
    }
    if ty.value_kind().is_none() || ty == Type::BigInt && !types.profile.bigint_values() {
        return Err(unsupported("class field value type"));
    }
    Ok((
        Field {
            name,
            ty,
            owner,
            access: Access::from_ast(property.accessibility),
            readonly: property.readonly,
        },
        property.value.clone(),
    ))
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
        ast::MethodKind::Method if method.function.is_async => Some("async class method"),
        ast::MethodKind::Method => None,
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
fn static_method(
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
