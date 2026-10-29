//! Type operators and built-in generic references of v32 `generics()`. Every
//! admitted form maps onto an existing closed type; forms that would need a new
//! value representation are rejected with the reason.
use super::*;
use ir::RecordField;

/// `keyof`, `readonly`, indexed access, intersections and `typeof` queries;
/// `None` for any other type expression.
pub(super) fn operator(ty: &ast::TsType, types: &mut ir::ValueTypes) -> Result<Option<Type>> {
    Ok(Some(match ty {
        ast::TsType::TsTypeOperator(operator) => match operator.op {
            ast::TsTypeOperatorOp::KeyOf => key_of(&operator.type_ann, types)?,
            ast::TsTypeOperatorOp::ReadOnly => {
                if !matches!(&*operator.type_ann, ast::TsType::TsArrayType(_)) {
                    return Err(unsupported("readonly operator on a non-array type"));
                }
                keyword_type(&operator.type_ann, types)?
            }
            ast::TsTypeOperatorOp::Unique => return Err(unsupported("unique symbol type")),
        },
        ast::TsType::TsIndexedAccessType(access) => indexed(access, types)?,
        ast::TsType::TsUnionOrIntersectionType(
            ast::TsUnionOrIntersectionType::TsIntersectionType(intersection),
        ) => intersect(intersection, types)?,
        ast::TsType::TsTypeQuery(_) => {
            return Err(unsupported(
                "typeof type query (module constants have no type-level identity)",
            ));
        }
        _ => return Ok(None),
    }))
}

/// `keyof R` of a closed record type: the string literal set of its fields.
fn key_of(target: &ast::TsType, types: &mut ir::ValueTypes) -> Result<Type> {
    match keyword_type(target, types)? {
        Type::Record(id) => {
            let names: Vec<String> = types.records[id as usize]
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect();
            if names.is_empty() {
                return Err(unsupported("keyof an empty object type (never)"));
            }
            literals::string_set(names.iter().map(String::as_str), types)
        }
        Type::Class(_) => Err(unsupported(
            "keyof a class type (methods and inherited members are keys)",
        )),
        _ => Err(unsupported("keyof a type other than a closed object type")),
    }
}

/// `R[K]` for a closed record and a string literal set, and `A[number]` for
/// an array type.
fn indexed(access: &ast::TsIndexedAccessType, types: &mut ir::ValueTypes) -> Result<Type> {
    let object = keyword_type(&access.obj_type, types)?;
    if let ast::TsType::TsKeywordType(keyword) = &*access.index_type
        && keyword.kind == ast::TsKeywordTypeKind::TsNumberKeyword
    {
        let Type::Array(id) = object else {
            return Err(unsupported("number index of a non-array type"));
        };
        return Ok(types.arrays[id as usize]);
    }
    let index = keyword_type(&access.index_type, types)?;
    let (Type::Record(id), Some(keys)) = (object, literals::string_members(index, types)) else {
        return Err(unsupported(
            "indexed access type other than a closed object type with string literal keys",
        ));
    };
    let mut members = Vec::new();
    for key in keys {
        let field = types.records[id as usize]
            .fields
            .iter()
            .find(|field| field.name == key)
            .ok_or_else(|| {
                type_error(format!("property {key} does not exist on the indexed type"))
            })?;
        members.push(field.ty);
    }
    if members.len() == 1 {
        Ok(members[0])
    } else {
        unions::intern(members, types)
    }
}

/// An intersection of types where each member is assignable to or from the
/// others collapses to the narrowest member (`keyof T & string`); any other
/// intersection would need a new record shape.
fn intersect(intersection: &ast::TsIntersectionType, types: &mut ir::ValueTypes) -> Result<Type> {
    let mut result: Option<Type> = None;
    for member in &intersection.types {
        let member = keyword_type(member, types)?;
        result = Some(match result {
            None => member,
            Some(current) if arguments::accepts(member, current, types) => current,
            Some(current) if arguments::accepts(current, member, types) => member,
            Some(_) => {
                return Err(unsupported(
                    "intersection of types that are not subtypes of each other",
                ));
            }
        });
    }
    result.ok_or_else(|| invalid("empty intersection type"))
}

fn exactly<'a>(
    reference: &'a ast::TsTypeRef,
    count: usize,
    name: &str,
) -> Result<&'a [Box<ast::TsType>]> {
    let params = reference
        .type_params
        .as_ref()
        .map_or(&[][..], |p| &p.params[..]);
    if params.len() != count {
        return Err(type_error(format!(
            "{name} expects {count} type argument(s), found {}",
            params.len()
        )));
    }
    Ok(params)
}

/// A type reference with type arguments: a declared generic type (which, like
/// TypeScript module scoping, shadows a built-in name) or a built-in generic.
pub(super) fn reference(reference: &ast::TsTypeRef, types: &mut ir::ValueTypes) -> Result<Type> {
    let ast::TsEntityName::Ident(id) = &reference.type_name else {
        return Err(unsupported("qualified generic type reference"));
    };
    let name = id.sym.as_ref();
    if named::declared(types, name) {
        return named::instantiate(name, reference.type_params.as_deref(), types);
    }
    match name {
        // Readonly is binding-level intent in Varta: the closed type is the same
        // and writes through it are not rejected (they behave as in JavaScript).
        "Readonly" => keyword_type(&exactly(reference, 1, name)?[0], types),
        "ReadonlyArray" => {
            let element = keyword_type(&exactly(reference, 1, name)?[0], types)?;
            arrays::intern(element, types)
        }
        "Partial" => partial(&exactly(reference, 1, name)?[0], types),
        "Record" => {
            let params = exactly(reference, 2, name)?;
            record(&params[0], &params[1], types)
        }
        "Promise" => Err(unsupported(
            "Promise<T> type (asynchronous values need the async suspension foundation)",
        )),
        _ => named::instantiate(name, reference.type_params.as_deref(), types),
    }
}

/// `Partial<R>` of a closed record: the same fields, all optional.
fn partial(target: &ast::TsType, types: &mut ir::ValueTypes) -> Result<Type> {
    let Type::Record(id) = keyword_type(target, types)? else {
        return Err(unsupported(
            "Partial of a type other than a closed object type",
        ));
    };
    if !types.profile.missing() {
        return Err(unsupported("Partial without optional fields"));
    }
    let mut fields = Vec::new();
    for field in types.records[id as usize].fields.clone() {
        let ty = if field.optional {
            field.ty
        } else {
            unions::intern(vec![field.ty, Type::Undefined], types)?
        };
        fields.push(RecordField {
            name: field.name,
            ty,
            optional: true,
        });
    }
    records::intern(fields, types)
}

/// `Record<K, V>` with a string literal key set: the closed record with one
/// required `V` field per key. Dynamic `string`/`number` keys need a map value.
fn record(key: &ast::TsType, value: &ast::TsType, types: &mut ir::ValueTypes) -> Result<Type> {
    let key = keyword_type(key, types)?;
    let Some(names) = literals::string_members(key, types) else {
        return Err(unsupported(
            "Record with non-literal keys (dynamic keys need a map value representation)",
        ));
    };
    let value = keyword_type(value, types)?;
    let fields = names
        .into_iter()
        .map(|name| {
            Ok(RecordField {
                name: records::key(name)?,
                ty: value,
                optional: false,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    records::intern(fields, types)
}
