//! Only explicit, literal entry-module declarations publish tools. No guest call
//! or object allocation is emitted for this compile-time form.
use super::*;
use crate::modules::Graph;
use std::{collections::BTreeSet, path::PathBuf};
use varta_common::{ToolCatalog, ToolContract, ToolLimits, ToolParameter, ToolType};

fn call(item: &ast::ModuleItem) -> Option<&ast::CallExpr> {
    let ast::ModuleItem::Stmt(ast::Stmt::Expr(stmt)) = item else {
        return None;
    };
    let ast::Expr::Call(call) = &*stmt.expr else {
        return None;
    };
    let ast::Callee::Expr(callee) = &call.callee else {
        return None;
    };
    let ast::Expr::Member(member) = &**callee else {
        return None;
    };
    if matches!(&*member.obj, ast::Expr::Ident(id) if id.sym == "varta")
        && matches!(&member.prop, ast::MemberProp::Ident(id) if id.sym == "tool")
    {
        Some(call)
    } else {
        None
    }
}
pub(super) fn is_declaration(item: &ast::ModuleItem) -> bool {
    call(item).is_some()
}

pub(super) fn collect(
    graph: &Graph,
    scopes: &BTreeMap<PathBuf, BTreeMap<String, Signature>>,
    functions: &[ir::Function],
    main: usize,
) -> Result<Vec<(usize, ToolContract)>> {
    let mut tools = BTreeMap::new();
    for path in &graph.order {
        let unit = &graph.units[path];
        for item in &unit.ast.body {
            let Some(call) = call(item) else { continue };
            let result = (|| {
                if path != &graph.entry {
                    return Err(invalid(
                        "varta.tool declarations belong in the entry module",
                    ));
                }
                let (name, description, handler) = declaration(call, &unit.source)?;
                if !unit
                    .exports
                    .iter()
                    .any(|export| export.from.is_none() && export.local == handler)
                {
                    return Err(invalid(
                        "tool handler must be an explicitly exported function binding",
                    ));
                }
                let signature = scopes[path]
                    .get(&handler)
                    .ok_or_else(|| invalid("unknown tool handler"))?;
                if signature.receiver.is_some() || signature.shape.extended() {
                    return Err(unsupported("typed receiver in Tool Contract v1"));
                }
                if signature.index == main {
                    return Err(invalid("main cannot be published as a tool"));
                }
                let parameters = signature
                    .names
                    .iter()
                    .zip(&signature.params)
                    .map(|(name, ty)| {
                        Ok(ToolParameter {
                            name: name.clone().ok_or_else(|| {
                                unsupported("destructured Tool Contract v1 parameter")
                            })?,
                            value_type: value_type(*ty)?,
                        })
                    })
                    .collect::<Result<_>>()?;
                let mut visited = BTreeSet::new();
                let mut effects = BTreeSet::new();
                effects_for(signature.index, functions, &mut visited, &mut effects);
                let tool = ToolContract {
                    export: format!("{}{}", varta_common::TOOL_EXPORT_PREFIX, name),
                    name: name.clone(),
                    description,
                    parameters,
                    result: value_type(signature.result)?,
                    required_effects: effects.into_iter().collect(),
                    limits: ToolLimits::default(),
                };
                ToolCatalog::new(vec![tool.clone()])
                    .validate()
                    .map_err(|e| invalid(e.to_string()))?;
                if tools.len() >= varta_common::MAX_TOOLS {
                    return Err(invalid("too many tools"));
                }
                if tools.insert(name, (signature.index, tool)).is_some() {
                    return Err(invalid("duplicate tool name"));
                }
                Ok(())
            })();
            result.map_err(|e| unit.source.locate(e, call.span))?;
        }
    }
    let tools: Vec<_> = tools.into_values().collect();
    if !tools.is_empty() {
        ToolCatalog::new(tools.iter().map(|(_, tool)| tool.clone()).collect())
            .to_bytes()
            .map_err(|e| {
                graph.units[&graph.entry]
                    .source
                    .locate(invalid(e.to_string()), graph.units[&graph.entry].ast.span())
            })?;
    }
    Ok(tools)
}

fn declaration(call: &ast::CallExpr, source: &Source) -> Result<(String, String, String)> {
    if call.type_args.is_some()
        || call.args.len() != 2
        || call.args.iter().any(|a| a.spread.is_some())
    {
        return Err(unsupported(
            "varta.tool requires literal metadata and a named handler",
        ));
    }
    let ast::Expr::Object(object) = &*call.args[0].expr else {
        return Err(unsupported("tool metadata must be an object literal"));
    };
    let mut fields = BTreeMap::new();
    for property in &object.props {
        let ast::PropOrSpread::Prop(property) = property else {
            return Err(unsupported("tool metadata spread"));
        };
        let ast::Prop::KeyValue(property) = &**property else {
            return Err(unsupported("tool metadata property"));
        };
        let key = match &property.key {
            ast::PropName::Ident(id) => id.sym.to_string(),
            ast::PropName::Str(value) => value
                .value
                .as_str()
                .ok_or_else(|| invalid("tool metadata must be UTF-8"))?
                .into(),
            _ => return Err(unsupported("computed tool metadata key")),
        };
        if !matches!(key.as_str(), "name" | "description") || fields.contains_key(&key) {
            return Err(invalid("unknown or duplicate tool metadata field"));
        }
        let ast::Expr::Lit(ast::Lit::Str(value)) = &*property.value else {
            return Err(unsupported("tool metadata value must be a string literal"));
        };
        let text = value
            .value
            .as_str()
            .ok_or_else(|| source.locate(invalid("tool metadata must be UTF-8"), value.span))?;
        let valid = if key == "name" {
            varta_common::valid_tool_name(text)
        } else {
            varta_common::valid_tool_description(text)
        };
        if !valid {
            return Err(source.locate(invalid(format!("invalid tool {key}")), value.span));
        }
        fields.insert(key, text.to_owned());
    }
    let name = fields
        .remove("name")
        .ok_or_else(|| invalid("tool name is required"))?;
    let description = fields
        .remove("description")
        .ok_or_else(|| invalid("tool description is required"))?;
    let ast::Expr::Ident(handler) = &*call.args[1].expr else {
        return Err(unsupported("tool handler must be a named function binding"));
    };
    Ok((name, description, handler.sym.to_string()))
}
fn value_type(ty: Type) -> Result<ToolType> {
    Ok(match ty {
        Type::Number => ToolType::Integer,
        Type::Boolean => ToolType::Boolean,
        Type::String => ToolType::String,
        Type::Void => ToolType::Void,
        _ => return Err(unsupported("non-primitive value in tool contract v1")),
    })
}

fn effects_for(
    index: usize,
    functions: &[ir::Function],
    visited: &mut BTreeSet<usize>,
    effects: &mut BTreeSet<String>,
) {
    if visited.insert(index) {
        statements(&functions[index].body, functions, visited, effects);
    }
}
pub(super) fn function_effects(index: usize, functions: &[ir::Function]) -> BTreeSet<String> {
    let mut effects = BTreeSet::new();
    effects_for(index, functions, &mut BTreeSet::new(), &mut effects);
    effects
}

fn expression(
    expr: &Expr,
    functions: &[ir::Function],
    visited: &mut BTreeSet<usize>,
    effects: &mut BTreeSet<String>,
) {
    match &expr.kind {
        E::ValueBlock { body, .. } => statements(body, functions, visited, effects),
        E::Assignment { value, body, .. } => {
            expression(value, functions, visited, effects);
            statements(body, functions, visited, effects);
        }
        E::ArrayDelete {
            array,
            index,
            error,
        } => {
            expression(array, functions, visited, effects);
            if let Some(index) = index {
                expression(index, functions, visited, effects);
            }
            if let Some(error) = error {
                expression(error, functions, visited, effects);
            }
        }
        E::FunctionCheck { value, .. } => expression(value, functions, visited, effects),
        E::RecordDelete { object, absent, .. } => {
            expression(object, functions, visited, effects);
            expression(absent, functions, visited, effects);
        }
        E::FunctionCall {
            callee,
            ignored_receiver,
            args,
            ..
        } => {
            expression(callee, functions, visited, effects);
            if let Some(value) = ignored_receiver {
                expression(value, functions, visited, effects);
            }
            for arg in args {
                expression(arg, functions, visited, effects);
            }
            for (index, function) in functions.iter().enumerate() {
                if visited.insert(index) {
                    statements(&function.body, functions, visited, effects);
                }
            }
        }
        E::StringSearch(_, value, needle, position) => {
            expression(value, functions, visited, effects);
            expression(needle, functions, visited, effects);
            expression(position, functions, visited, effects);
        }
        E::StringRange {
            value, start, end, ..
        } => {
            expression(value, functions, visited, effects);
            expression(start, functions, visited, effects);
            expression(end, functions, visited, effects);
        }
        E::StringTrim { value, .. } => {
            expression(value, functions, visited, effects);
        }
        E::StringRepeat {
            value,
            count,
            error,
            ..
        } => {
            expression(value, functions, visited, effects);
            expression(count, functions, visited, effects);
            expression(error, functions, visited, effects);
        }
        E::Conditional(c, a, b) => {
            expression(c, functions, visited, effects);
            expression(a, functions, visited, effects);
            expression(b, functions, visited, effects);
        }
        E::UnionLogical(_, a, b) => {
            expression(a, functions, visited, effects);
            expression(b, functions, visited, effects);
        }
        E::NewError(_, a)
        | E::ErrorName(a)
        | E::ErrorMessage(a)
        | E::ErrorText(a)
        | E::ErrorOwn(a)
        | E::BoxUnion(a)
        | E::IsError(a)
        | E::IsClass { value: a, .. }
        | E::ArrayKind(a, _)
        | E::IsUnionMember(a, _)
        | E::IsUndefined(a)
        | E::IsNullish(a)
        | E::UnboxUnion(a) => expression(a, functions, visited, effects),
        E::ArrayEnumerate { value, .. } => expression(value, functions, visited, effects),
        E::ArrayReserve { length, .. } => expression(length, functions, visited, effects),
        E::Array(values) => {
            for value in values.iter().flatten() {
                expression(value, functions, visited, effects);
            }
        }
        E::Record(fields) | E::OptionalRecord(fields, _) => {
            for (_, value) in fields {
                expression(value, functions, visited, effects);
            }
        }
        E::Math(_, args) | E::NumberPredicate(_, args) => {
            for arg in args {
                expression(arg, functions, visited, effects);
            }
        }
        E::CheckedBigIntHost { host, args, .. } | E::Host(host, args) => {
            if let Some(id) = host.capability() {
                effects.insert(id.into());
            }
            for arg in args {
                expression(arg, functions, visited, effects);
            }
        }
        E::Call(index, args) => {
            effects_for(*index, functions, visited, effects);
            for arg in args {
                expression(arg, functions, visited, effects);
            }
        }
        E::ArraySearch(_, a, b, c) | E::ArraySlice(a, b, c) => {
            expression(a, functions, visited, effects);
            expression(b, functions, visited, effects);
            expression(c, functions, visited, effects);
        }
        E::ArrayFill(a, b, c, d) => {
            expression(a, functions, visited, effects);
            expression(b, functions, visited, effects);
            expression(c, functions, visited, effects);
            expression(d, functions, visited, effects);
        }
        E::ArrayAt(a, b)
        | E::ArrayHasOwn(a, b)
        | E::ArrayStringHasOwn(a, b)
        | E::ArrayPush(a, b)
        | E::MaybeIndex(a, b)
        | E::NumberSameValue(a, b)
        | E::Binary(_, a, b)
        | E::Index(a, b)
        | E::Sequence(a, b) => {
            expression(a, functions, visited, effects);
            expression(b, functions, visited, effects);
        }
        E::ArrayPop(a)
        | E::HasOwn(a, _)
        | E::Negative(a)
        | E::Undefined(a)
        | E::ToBoolean(a)
        | E::Not(a)
        | E::Capture(a)
        | E::ResultField(_, a)
        | E::Field(a, _)
        | E::Json(a, _)
        | E::ParseJson(a)
        | E::Length(a) => expression(a, functions, visited, effects),
        _ => {}
    }
}
fn statements(
    body: &[S],
    functions: &[ir::Function],
    visited: &mut BTreeSet<usize>,
    effects: &mut BTreeSet<String>,
) {
    for statement in body {
        match statement {
            S::RecordEnumerate { values, .. } => {
                for value in values {
                    expression(value, functions, visited, effects);
                }
            }
            S::Throw(e) | S::Init(_, e) | S::Set(_, e) | S::Expr(e) | S::Return(Some(e)) => {
                expression(e, functions, visited, effects)
            }
            S::ArrayLength {
                array,
                length,
                error,
            } => {
                expression(array, functions, visited, effects);
                expression(length, functions, visited, effects);
                expression(error, functions, visited, effects);
            }
            S::SetField(a, _, b) => {
                expression(a, functions, visited, effects);
                expression(b, functions, visited, effects);
            }
            S::SetIndex {
                array: a,
                index: i,
                value: b,
                ..
            } => {
                expression(a, functions, visited, effects);
                expression(i, functions, visited, effects);
                expression(b, functions, visited, effects);
            }
            S::TryCatch { body, handler, .. } => {
                statements(body, functions, visited, effects);
                statements(handler, functions, visited, effects);
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                statements(body, functions, visited, effects);
                statements(finalizer, functions, visited, effects);
            }
            S::Block(b) | S::Scope(_, b) | S::Label { body: b, .. } => {
                statements(b, functions, visited, effects)
            }
            S::If(e, a, b) => {
                expression(e, functions, visited, effects);
                statements(a, functions, visited, effects);
                statements(b, functions, visited, effects);
            }
            S::While(e, b) | S::DoWhile(e, b) => {
                expression(e, functions, visited, effects);
                statements(b, functions, visited, effects);
            }
            S::For(a, e, u, b) => {
                statements(a, functions, visited, effects);
                if let Some(e) = e {
                    expression(e, functions, visited, effects);
                }
                statements(u, functions, visited, effects);
                statements(b, functions, visited, effects);
            }
            S::Switch { value, cases, .. } => {
                expression(value, functions, visited, effects);
                for (e, b) in cases {
                    if let Some(e) = e {
                        expression(e, functions, visited, effects);
                    }
                    statements(b, functions, visited, effects);
                }
            }
            _ => {}
        }
    }
}
