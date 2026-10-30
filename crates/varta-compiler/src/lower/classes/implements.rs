//! `class C implements I, J` (v37 `class_generics`): a compile-time
//! conformance check with no runtime effect. Each clause is resolved like the
//! annotation `I<…>` in the class's module and must be a closed object type
//! (an interface or alias, whose method signatures are function fields). Every
//! member must be provided by a public instance field, getter or method of the
//! class (own or inherited) of an assignable type; optional members may be
//! missing. Class instances stay nominal: `implements` does not make a class
//! instance assignable to the interface.
use super::*;

/// Check the `implements` clauses of the laid-out class `id`.
pub(super) fn check(
    id: u32,
    class: &ast::Class,
    source: &Source,
    types: &mut ir::ValueTypes,
) -> Result<()> {
    for clause in &class.implements {
        let ast::Expr::Ident(name) = &*clause.expr else {
            return Err(source.locate(
                unsupported("implements of a qualified or computed name"),
                clause.span,
            ));
        };
        let reference = ast::TsType::TsTypeRef(ast::TsTypeRef {
            span: clause.span,
            type_name: ast::TsEntityName::Ident(name.clone()),
            type_params: clause.type_args.clone(),
        });
        let ty = keyword_type(&reference, types).map_err(|e| source.locate(e, clause.span))?;
        let fields = match ty {
            Type::Record(record) => types.records[record as usize].fields.clone(),
            Type::Class(_) => {
                return Err(source.locate(
                    unsupported(&format!(
                        "class implements class {}; class instance types are nominal",
                        name.sym
                    )),
                    clause.span,
                ));
            }
            other => {
                return Err(source.locate(
                    unsupported(&format!(
                        "class implements {} ({other}), which is not an interface or closed object type",
                        name.sym
                    )),
                    clause.span,
                ));
            }
        };
        for field in &fields {
            conforms(id, field, types).map_err(|reason| {
                source.locate(
                    type_error(format!(
                        "class {} incorrectly implements {}: {reason}",
                        types.classes.get(id).name,
                        name.sym
                    )),
                    clause.span,
                )
            })?;
        }
    }
    Ok(())
}

/// Why class `id` does not provide the interface member `field`, if it does not.
fn conforms(
    id: u32,
    field: &ir::RecordField,
    types: &ir::ValueTypes,
) -> std::result::Result<(), String> {
    let classes = &types.classes;
    let name = &field.name;
    let public = |access: Access| {
        if access == Access::Public {
            Ok(())
        } else {
            Err(format!("member {name} is not public in the class"))
        }
    };
    if let Some((_, own)) = classes.field(id, name) {
        public(own.access)?;
        return assignable(field.ty, own.ty, types)
            .then_some(())
            .ok_or_else(|| format!("field {name} has type {}, not {}", own.ty, field.ty));
    }
    let info = classes.get(id);
    if let Some(method) = info.methods.get(name) {
        public(method.access)?;
        let signature = &method.signature;
        if method.getter {
            return assignable(field.ty, signature.result, types)
                .then_some(())
                .ok_or_else(|| {
                    format!(
                        "getter {name} returns {}, not {}",
                        signature.result, field.ty
                    )
                });
        }
        let callable =
            unions::members(field.ty, types)
                .into_iter()
                .find_map(|member| match member {
                    Type::Function(function) => Some(types.functions[function as usize].clone()),
                    _ => None,
                });
        let Some(wanted) = callable else {
            return Err(format!("method {name} is not a {}", field.ty));
        };
        let matches = wanted.receiver.is_none()
            && wanted.shape == signature.shape
            && wanted.params.len() == signature.params.len()
            && wanted
                .params
                .iter()
                .zip(&signature.params)
                .all(|(given, param)| assignable(*param, *given, types))
            && assignable(wanted.result, signature.result, types);
        return matches
            .then_some(())
            .ok_or_else(|| format!("method {name} does not match {}", field.ty));
    }
    if info.generic_methods.contains_key(name) {
        return Err(format!(
            "generic method {name} cannot implement the non-generic member {}",
            field.ty
        ));
    }
    if field.optional {
        return Ok(());
    }
    Err(format!("member {name} is missing"))
}

fn assignable(expected: Type, actual: Type, types: &ir::ValueTypes) -> bool {
    arguments::accepts(expected, actual, types)
}
