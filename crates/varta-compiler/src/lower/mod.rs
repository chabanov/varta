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
    /// Bound to an `as const` value (v32): writes through it are rejected.
    readonly: bool,
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
    /// Generator type id while lowering a `function*` body (v33).
    generator: Option<u32>,
    /// Promise type id of the async function whose body is lowered (v34).
    asynchronous: Option<u32>,
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
    if types.profile.generics()
        && let Some(ty) = generics::parameter(ty, types)
    {
        return Ok(ty);
    }
    if types.profile.named_types()
        && let ast::TsType::TsTypeRef(reference) = ty
        && reference.type_params.is_none()
        && let ast::TsEntityName::Ident(id) = &reference.type_name
        && named::declared(types, id.sym.as_ref())
    {
        return named::resolve(id.sym.as_ref(), types);
    }
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
    if types.profile.promises()
        && let ast::TsType::TsTypeRef(reference) = ty
        && matches!(&reference.type_name, ast::TsEntityName::Ident(id) if id.sym == "Promise")
        && !named::declared(types, "Promise")
    {
        return promises::annotation(reference, types);
    }
    if types.profile.generators()
        && let ast::TsType::TsTypeRef(reference) = ty
        && matches!(&reference.type_name, ast::TsEntityName::Ident(id) if id.sym == "Generator")
        && !named::declared(types, "Generator")
    {
        return generators::annotation(reference, types);
    }
    if types.profile.async_generators()
        && let ast::TsType::TsTypeRef(reference) = ty
        && let ast::TsEntityName::Ident(id) = &reference.type_name
        && promises::async_generator_name(id.sym.as_ref())
        && !named::declared(types, id.sym.as_ref())
    {
        return promises::async_generator_annotation(reference, id.sym.as_ref(), types);
    }
    if types.profile.generators()
        && let ast::TsType::TsTypeRef(reference) = ty
        && matches!(&reference.type_name, ast::TsEntityName::Ident(id) if id.sym == "IteratorResult")
        && !named::declared(types, "IteratorResult")
    {
        return generators::iterator_result(reference, types);
    }
    match ty {
        ast::TsType::TsUnionOrIntersectionType(ast::TsUnionOrIntersectionType::TsUnionType(
            union,
        )) => {
            if types.profile.literal_types() {
                return literals::union(union, types);
            }
            if union.types.len() > literals::MAX_UNION_MEMBERS {
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
        ast::TsType::TsLitType(literal) if types.profile.literal_types() => {
            return literals::annotation(literal, types);
        }
        _ => {}
    }
    if let ast::TsType::TsTypeLit(literal) = ty {
        return records::type_literal(literal, types);
    }
    if types.profile.named_types()
        && let ast::TsType::TsTypeRef(reference) = ty
        && let ast::TsEntityName::Ident(id) = &reference.type_name
    {
        if reference.type_params.is_some() {
            if types.profile.generics() {
                return type_operators::reference(reference, types);
            }
            return Err(unsupported("generic type reference"));
        }
        return named::resolve(id.sym.as_ref(), types);
    }
    if types.profile.generics()
        && let Some(ty) = type_operators::operator(ty, types)?
    {
        return Ok(ty);
    }
    let ast::TsType::TsKeywordType(keyword) = ty else {
        return Err(unsupported("type annotation"));
    };
    Ok(match keyword.kind {
        ast::TsKeywordTypeKind::TsNumberKeyword => types.number(),
        ast::TsKeywordTypeKind::TsBooleanKeyword => Type::Boolean,
        ast::TsKeywordTypeKind::TsStringKeyword => Type::String,
        ast::TsKeywordTypeKind::TsBigIntKeyword if types.profile.bigint() => Type::BigInt,
        ast::TsKeywordTypeKind::TsVoidKeyword => Type::Void,
        ast::TsKeywordTypeKind::TsNullKeyword => Type::Null,
        ast::TsKeywordTypeKind::TsUndefinedKeyword if types.profile.has_undefined() => {
            Type::Undefined
        }
        ast::TsKeywordTypeKind::TsUnknownKeyword if types.profile.unknown_type() => Type::Unknown,
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
mod array_inference;
mod array_iteration;
mod array_mutation;
mod arrays;
mod assignment_patterns;
mod assignment_values;
mod bigint;
mod bigint_values;
mod bindings;
mod bitwise;
mod callback_inference;
pub(crate) mod classes;
mod completion;
mod compound_assignments;
mod declarations;
mod declared_fields;
mod dictionaries;
mod dictionary_helpers;
mod dictionary_spread;
mod dictionary_updates;
mod errors;
mod exceptions;
mod exports;
mod expr;
mod flow;
mod flow_call_ast;
mod flow_calls;
mod flow_control;
mod flow_loops;
mod flow_parameters;
mod for_in;
mod for_of;
mod function_expressions;
mod function_fields;
mod functions;
mod generators;
mod generic_calls;
pub(crate) mod generics;
mod guest_logical;
mod http;
mod interfaces;
pub(crate) mod json;
mod labels;
mod link;
mod literal_erasure;
pub(crate) mod literals;
mod logical_assignments;
mod math;
mod math_hypot;
mod missing;
mod mixed_records;
mod module_constants;
pub(crate) mod named;
mod net;
mod number_format;
mod number_parse;
mod number_predicates;
mod number_radix;
mod numeric_constants;
mod object_assign;
mod object_entries;
mod object_enumeration;
mod object_is;
mod object_narrowing;
mod optional_chains;
mod own_keys;
mod parameters;
mod patterns;
mod power;
mod primitive_arithmetic;
mod primitive_builtins;
mod primitive_comparison;
mod primitive_string;
mod primitive_unary;
pub(crate) mod promises;
mod property_deletion;
mod record_reads;
mod record_writes;
mod records;
mod reduce_inference;
mod references;
mod results;
mod signature;
mod spread;
mod stmt;
mod string_enumeration;
mod strings;
pub(crate) mod suspension;
mod templates;
mod tools;
mod type_operators;
mod undefined;
mod unions;
mod unknown;
mod updates;
mod web_text;

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
                    readonly: false,
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
