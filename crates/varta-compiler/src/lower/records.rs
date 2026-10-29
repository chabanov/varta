use super::*;
use ir::{RecordField, RecordShape};
use varta_common::value::{MAX_RECORD_KEY_BYTES, MAX_RECORD_SHAPES};

pub(super) fn key(name: String) -> Result<String> {
    if name.len() > MAX_RECORD_KEY_BYTES {
        return Err(invalid("object key exceeds byte limit"));
    }
    if name == "__proto__" {
        return Err(unsupported("object prototype initializer"));
    }
    Ok(name)
}
pub(super) fn expression_key(expr: &ast::Expr) -> Result<String> {
    key(match expr {
        ast::Expr::Ident(id) => id.sym.to_string(),
        ast::Expr::Lit(ast::Lit::Str(s)) => s
            .value
            .as_str()
            .ok_or_else(|| unsupported("non-UTF-8 object key"))?
            .into(),
        ast::Expr::Lit(ast::Lit::Num(n))
            if n.value.fract() == 0.0 && n.value >= 0.0 && n.value <= u32::MAX as f64 =>
        {
            format!("{}", n.value as u32)
        }
        _ => return Err(unsupported("dynamic object key")),
    })
}
pub(super) const INHERITED_NAMES: &[&str] = &[
    "constructor",
    "__defineGetter__",
    "__defineSetter__",
    "hasOwnProperty",
    "__lookupGetter__",
    "__lookupSetter__",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "toLocaleString",
    "toString",
    "valueOf",
    "__proto__",
];
pub(super) fn inherited(name: &str) -> bool {
    INHERITED_NAMES.contains(&name)
}
pub(super) fn property_key(name: &ast::PropName) -> Result<String> {
    match name {
        ast::PropName::Ident(id) => key(id.sym.to_string()),
        ast::PropName::Str(s) => key(s
            .value
            .as_str()
            .ok_or_else(|| unsupported("non-UTF-8 object key"))?
            .into()),
        ast::PropName::Num(n) => expression_key(&ast::Expr::Lit(ast::Lit::Num(n.clone()))),
        _ => Err(unsupported("computed object initializer")),
    }
}
pub(super) fn intern(mut fields: Vec<RecordField>, types: &mut ir::ValueTypes) -> Result<Type> {
    if fields.iter().any(|f| matches!(f.ty, Type::Generator(_))) {
        return Err(generators::opaque("an object"));
    }
    if fields
        .iter()
        .any(|field| matches!(field.ty, Type::Function(_)))
    {
        return Err(unsupported("function-valued record fields"));
    }
    if fields.len() > types.max_record_fields() {
        return Err(invalid("object field count exceeds limit"));
    }
    fields.sort_by(|a, b| a.name.cmp(&b.name));
    if fields.windows(2).any(|p| p[0].name == p[1].name) {
        return Err(unsupported("duplicate object key"));
    }
    if fields.iter().any(|f| {
        f.ty.value_kind().is_none() || f.ty == Type::BigInt && !types.profile.bigint_values()
    }) {
        return Err(unsupported("object field value type"));
    }
    let shape = RecordShape {
        fields,
        class: None,
    };
    if let Some(index) = types.records.iter().position(|record| record == &shape) {
        return Ok(Type::Record(index as u32));
    }
    if types.records.len() >= MAX_RECORD_SHAPES {
        return Err(invalid("object shape count exceeds limit"));
    }
    let index = types.records.len() as u32;
    types.records.push(shape);
    Ok(Type::Record(index))
}
pub(super) fn type_literal(literal: &ast::TsTypeLit, types: &mut ir::ValueTypes) -> Result<Type> {
    if types.profile.index_signatures()
        && let Some(ty) = dictionaries::signature(literal, types)?
    {
        return Ok(ty);
    }
    let fields = type_fields(&literal.members, types)?;
    intern(fields, types)
}
/// The checked fields of closed object type members, in declaration order and
/// not yet interned (interface `extends` combines them first).
pub(super) fn type_fields(
    members: &[ast::TsTypeElement],
    types: &mut ir::ValueTypes,
) -> Result<Vec<RecordField>> {
    if members.len() > types.max_record_fields() {
        return Err(invalid("object field count exceeds limit"));
    }
    let mut fields = Vec::new();
    for member in members {
        let ast::TsTypeElement::TsPropertySignature(field) = member else {
            return Err(unsupported("object type member"));
        };
        if field.optional && !types.profile.missing() || field.readonly || field.computed {
            return Err(unsupported("optional, readonly or computed object field"));
        }
        let name = expression_key(&field.key)?;
        if field.optional && inherited(&name) {
            return Err(unsupported(
                "optional inherited property requires prototype semantics",
            ));
        }
        let mut ty = annotation(field.type_ann.as_deref(), types)?
            .ok_or_else(|| invalid("object fields require annotations"))?;
        if field.optional {
            ty = unions::intern(vec![ty, Type::Undefined], types)?;
        }
        fields.push(RecordField {
            name,
            ty,
            optional: field.optional,
        });
    }
    Ok(fields)
}

impl Lower<'_> {
    pub(super) fn record(
        &mut self,
        object: &ast::ObjectLit,
        expected: Option<Type>,
    ) -> Result<Expr> {
        if let Some(Type::Dictionary(id)) = expected {
            return self.dictionary_literal(object, id);
        }
        if object.props.len() > self.types.max_record_fields() {
            return Err(invalid("object field count exceeds limit"));
        }
        let mut values = Vec::new();
        for property in &object.props {
            let ast::PropOrSpread::Prop(property) = property else {
                return Err(unsupported("object spread"));
            };
            let (name, value) = match &**property {
                ast::Prop::KeyValue(field) => {
                    let name = property_key(&field.key)?;
                    let ty = match expected {
                        Some(Type::Record(id)) => self.types.records[id as usize]
                            .fields
                            .iter()
                            .find(|f| f.name == name)
                            .map(|f| f.ty),
                        _ => None,
                    };
                    (name, self.expr_expected(&field.value, ty)?)
                }
                ast::Prop::Shorthand(id) => {
                    let name = key(id.sym.to_string())?;
                    let ty = match expected {
                        Some(Type::Record(id)) => self.types.records[id as usize]
                            .fields
                            .iter()
                            .find(|f| f.name == name)
                            .map(|f| f.ty),
                        _ => None,
                    };
                    (name, self.expr_expected(&ast::Expr::Ident(id.clone()), ty)?)
                }
                _ => return Err(unsupported("object method, getter or setter")),
            };
            values.push((name, value));
        }
        if self.types.profile.missing()
            && let Some(Type::Record(id)) = expected
            && self.types.records[id as usize]
                .fields
                .iter()
                .any(|f| f.optional)
        {
            return self.optional_record(object.span, id, values);
        }
        let ty = intern(
            values
                .iter()
                .map(|(name, value)| RecordField {
                    name: name.clone(),
                    ty: value.ty,
                    optional: false,
                })
                .collect(),
            self.types,
        )?;
        let Type::Record(index) = ty else {
            return Err(invalid("missing object type"));
        };
        let shape = &self.types.records[index as usize];
        let fields = values
            .into_iter()
            .map(|(name, value)| {
                let index = shape
                    .fields
                    .iter()
                    .position(|field| field.name == name)
                    .ok_or_else(|| invalid("missing object field"))?;
                Ok((index as u32, value))
            })
            .collect::<Result<_>>()?;
        Ok(Expr {
            span: object.span,
            ty,
            kind: E::Record(fields),
        })
    }
    pub(super) fn record_field(
        &mut self,
        member: &ast::MemberExpr,
        value: Expr,
        read: bool,
    ) -> Result<Expr> {
        let Type::Record(index) = value.ty else {
            return Err(unsupported("object property on a non-object"));
        };
        let name = match &member.prop {
            ast::MemberProp::Ident(id) => id.sym.to_string(),
            ast::MemberProp::Computed(key)
                if matches!(
                    &*key.expr,
                    ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                ) =>
            {
                expression_key(&key.expr)?
            }
            ast::MemberProp::Computed(key) if read && self.types.profile.closures() => {
                let key = self.expr(&key.expr)?;
                if key.ty != Type::String {
                    return Err(self.source.locate(
                        type_error("computed record key must be a narrowed string; coercion is unsupported"),
                        key.span,
                    ));
                }
                return self.dynamic_record_read(member.span, value, key, index);
            }
            ast::MemberProp::Computed(_) => return Err(unsupported("dynamic object property")),
            _ => return Err(unsupported("private object property")),
        };
        let found = self.types.records[index as usize]
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.name == name);
        let Some((slot, field)) = found else {
            if read && self.types.profile.missing() && !inherited(&name) {
                return Ok(Expr {
                    span: member.span,
                    ty: Type::Undefined,
                    kind: E::Undefined(Box::new(value)),
                });
            }
            return Err(self.source.locate(
                invalid(format!("unknown or inherited object property: {name}")),
                member.prop.span(),
            ));
        };
        Ok(Expr {
            span: member.span,
            ty: field.ty,
            kind: E::Field(Box::new(value), slot as u32),
        })
    }
}
