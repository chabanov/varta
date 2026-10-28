//! Resolve lexical bindings and types before emitting any Wasm instructions.
use crate::source::Source;
use crate::{
    CompilerError,
    ir::{self, Expr, Expression as E, Host, Statement as S, Type},
};
use std::collections::BTreeMap;
use swc_common::Spanned;
use swc_ecma_ast as ast;

type Result<T> = std::result::Result<T, CompilerError>;
fn invalid(message: impl Into<String>) -> CompilerError {
    CompilerError::Invalid(message.into())
}
fn type_error(message: impl Into<String>) -> CompilerError {
    CompilerError::Type(message.into())
}
fn unsupported(kind: &str) -> CompilerError {
    CompilerError::Unsupported {
        kind: kind.into(),
        detail: "outside the supported TypeScript subset".into(),
    }
}

#[derive(Clone)]
struct Signature {
    shape: ir::ParameterShape,
    receiver: Option<Type>,
    index: usize,
    names: Vec<Option<String>>,
    params: Vec<Type>,
    result: Type,
}
#[derive(Clone)]
struct Binding {
    identity: (usize, u32),
    slot: u32,
    ty: Option<Type>,
    initialized: bool,
    mutable: bool,
}
struct Lower<'a> {
    source: &'a Source,
    types: &'a mut ir::ValueTypes,
    functions: &'a BTreeMap<String, Signature>,
    scopes: Vec<BTreeMap<String, Binding>>,
    next: u32,
    local_types: BTreeMap<u32, Type>,
    refinements: BTreeMap<u32, Type>,
    writes: BTreeMap<u32, u64>,
    call_writes: std::collections::BTreeSet<(usize, u32)>,
    call_unknown: bool,
    direct_writes: std::collections::BTreeSet<u32>,
    call_parameters: std::collections::BTreeSet<(usize, u32)>,
    parameter_values: BTreeMap<(usize, u32), (usize, u32)>,
    parameter_roots: BTreeMap<u32, (u32, bool)>,
    known_calls: BTreeMap<(usize, u32), std::collections::BTreeSet<usize>>,
    call_effects: &'a mut BTreeMap<usize, flow_calls::Impact>,
    reachable: bool,
    control: flow_control::Control,
    guard_blocked: std::collections::BTreeSet<u32>,
    outer_narrows: BTreeMap<(usize, u32), Type>,
    result: Type,
    infer_returns: bool,
    entry: usize,
    loops: usize,
    labels: Vec<(String, u32, bool)>,
    next_label: u32,
    switches: usize,
    depth: usize,
    repeat_slots: Option<(u32, u32)>,
    generated: &'a mut BTreeMap<usize, ir::Function>,
    next_function: &'a mut usize,
    index: usize,
    outer: BTreeMap<String, Binding>,
    captures: BTreeMap<(usize, u32), Binding>,
    cells: std::collections::BTreeSet<u32>,
    guarded: std::collections::BTreeSet<u32>,
}

pub(crate) use link::lower;

fn annotation(ann: Option<&ast::TsTypeAnn>, types: &mut ir::ValueTypes) -> Result<Option<Type>> {
    let Some(ann) = ann else {
        return Ok(None);
    };
    if let ast::TsType::TsTypeRef(reference) = &*ann.type_ann
        && matches!(&reference.type_name, ast::TsEntityName::Ident(id) if id.sym == "VartaResult")
    {
        let params = reference
            .type_params
            .as_ref()
            .ok_or_else(|| unsupported("VartaResult without a value type"))?;
        if params.params.len() != 1 {
            return Err(unsupported("VartaResult type argument count"));
        }
        return keyword_type(&params.params[0], types)?
            .captured()
            .map(Some)
            .ok_or_else(|| unsupported("nested VartaResult type"));
    }
    Ok(Some(keyword_type(&ann.type_ann, types)?))
}
fn keyword_type(ty: &ast::TsType, types: &mut ir::ValueTypes) -> Result<Type> {
    if let ast::TsType::TsTypeRef(reference) = ty
        && reference.type_params.is_none()
        && let ast::TsEntityName::Ident(id) = &reference.type_name
        && let Some(class) = varta_common::value::ErrorClass::from_name(id.sym.as_ref())
    {
        return errors::annotation(class, types);
    }
    if let ast::TsType::TsTypeRef(reference) = ty
        && reference.type_params.is_none()
        && let ast::TsEntityName::Ident(id) = &reference.type_name
        && let Some(ty) = http::builtin(id.sym.as_ref(), types)?
    {
        return Ok(ty);
    }
    if let ast::TsType::TsFnOrConstructorType(ast::TsFnOrConstructorType::TsFnType(function)) = ty {
        return functions::type_annotation(function, types);
    }
    match ty {
        ast::TsType::TsUnionOrIntersectionType(ast::TsUnionOrIntersectionType::TsUnionType(
            union,
        )) => {
            if union.types.len() > 16 {
                return Err(invalid("union member count exceeds limit"));
            }
            let members = union
                .types
                .iter()
                .map(|ty| keyword_type(ty, types))
                .collect::<Result<Vec<_>>>()?;
            return unions::intern(members, types);
        }
        ast::TsType::TsArrayType(array) => {
            let element = keyword_type(&array.elem_type, types)?;
            return arrays::intern(element, types);
        }
        ast::TsType::TsParenthesizedType(parenthesized) => {
            return keyword_type(&parenthesized.type_ann, types);
        }
        ast::TsType::TsTypeRef(reference) if matches!(&reference.type_name, ast::TsEntityName::Ident(id) if id.sym == "Array") =>
        {
            let params = reference
                .type_params
                .as_ref()
                .filter(|p| p.params.len() == 1)
                .ok_or_else(|| unsupported("Array requires one element type"))?;
            let element = keyword_type(&params.params[0], types)?;
            return arrays::intern(element, types);
        }
        _ => {}
    }
    if let ast::TsType::TsTypeLit(literal) = ty {
        return records::type_literal(literal, types);
    }
    let ast::TsType::TsKeywordType(keyword) = ty else {
        return Err(unsupported("type annotation"));
    };
    Ok(match keyword.kind {
        ast::TsKeywordTypeKind::TsNumberKeyword => types.number(),
        ast::TsKeywordTypeKind::TsBooleanKeyword => Type::Boolean,
        ast::TsKeywordTypeKind::TsStringKeyword => Type::String,
        ast::TsKeywordTypeKind::TsVoidKeyword => Type::Void,
        ast::TsKeywordTypeKind::TsNullKeyword => Type::Null,
        ast::TsKeywordTypeKind::TsUndefinedKeyword if types.profile.has_undefined() => {
            Type::Undefined
        }
        _ => return Err(unsupported("type annotation")),
    })
}
fn scalar(ty: Type) -> Result<()> {
    if matches!(ty, Type::Number | Type::Float64 | Type::Boolean) {
        Ok(())
    } else {
        Err(unsupported("operand requiring number or boolean"))
    }
}
fn value_type(ty: Type) -> Result<()> {
    if ty == Type::Void {
        Err(unsupported("void value"))
    } else {
        Ok(())
    }
}
fn contains_value_return(body: &[ast::Stmt]) -> bool {
    body.iter().any(|s| match s {
        ast::Stmt::Return(ret) => ret.arg.is_some(),
        ast::Stmt::Try(t) => {
            contains_value_return(&t.block.stmts)
                || t.handler
                    .as_ref()
                    .is_some_and(|h| contains_value_return(&h.body.stmts))
                || t.finalizer
                    .as_ref()
                    .is_some_and(|b| contains_value_return(&b.stmts))
        }
        ast::Stmt::Block(block) => contains_value_return(&block.stmts),
        ast::Stmt::If(branch) => {
            contains_value_return(std::slice::from_ref(&branch.cons))
                || branch
                    .alt
                    .as_ref()
                    .is_some_and(|s| contains_value_return(std::slice::from_ref(s)))
        }
        _ => false,
    })
}
fn returns(body: &[S]) -> bool {
    completion::exits_function(body)
}

mod arguments;
mod array_callbacks;
mod array_iteration;
mod array_mutation;
mod arrays;
mod assignment_patterns;
mod assignment_values;
mod bindings;
mod bitwise;
mod callback_inference;
mod completion;
mod compound_assignments;
mod declarations;
mod errors;
mod exceptions;
mod expr;
mod flow;
mod flow_call_ast;
mod flow_calls;
mod flow_control;
mod flow_loops;
mod flow_parameters;
mod for_of;
mod function_expressions;
mod functions;
mod guest_logical;
mod http;
mod json;
mod labels;
mod link;
mod logical_assignments;
mod math;
mod math_hypot;
mod missing;
mod number_format;
mod number_parse;
mod number_predicates;
mod number_radix;
mod numeric_constants;
mod object_enumeration;
mod own_keys;
mod parameters;
mod patterns;
mod power;
mod primitive_builtins;
mod primitive_string;
mod record_reads;
mod record_writes;
mod records;
mod reduce_inference;
mod references;
mod results;
mod signature;
mod spread;
mod stmt;
mod strings;
mod tools;
mod undefined;
mod unions;
mod updates;

impl Lower<'_> {
    fn declare(
        &mut self,
        name: &str,
        ty: Option<Type>,
        initialized: bool,
        mutable: bool,
    ) -> Result<()> {
        if matches!(name, "varta" | "console" | "log" | "JSON") || self.functions.contains_key(name)
        {
            return Err(invalid(format!("reserved binding: {name}")));
        }
        let slot = self.next;
        self.next += 1;
        if let Some(ty) = ty {
            self.local_types.insert(slot, ty);
        }
        if self
            .scopes
            .last_mut()
            .ok_or_else(|| invalid("missing lexical scope"))?
            .insert(
                name.into(),
                Binding {
                    identity: (self.index, slot),
                    slot,
                    ty,
                    initialized,
                    mutable,
                },
            )
            .is_some()
        {
            return Err(invalid(format!("duplicate binding: {name}")));
        }
        Ok(())
    }
    fn binding(&self, name: &str) -> Result<Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name))
            .cloned()
            .or_else(|| {
                self.outer.get(name).map(|binding| {
                    self.captures
                        .get(&binding.identity)
                        .unwrap_or(binding)
                        .clone()
                })
            })
            .ok_or_else(|| invalid(format!("unknown identifier: {name}")))
    }
    fn update(&mut self, name: &str, ty: Type) -> Result<()> {
        let binding = self
            .scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(name))
            .ok_or_else(|| invalid("missing binding"))?;
        binding.ty = Some(ty);
        binding.initialized = true;
        self.local_types.insert(binding.slot, ty);
        Ok(())
    }
}
