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
    index: usize,
    params: Vec<Type>,
    result: Type,
}
#[derive(Clone)]
struct Binding {
    slot: u32,
    ty: Option<Type>,
    initialized: bool,
    mutable: bool,
}
struct Lower<'a> {
    source: &'a Source,
    functions: &'a BTreeMap<String, Signature>,
    scopes: Vec<BTreeMap<String, Binding>>,
    next: u32,
    local_types: BTreeMap<u32, Type>,
    result: Type,
    entry: usize,
    loops: usize,
    switches: usize,
    depth: usize,
}

pub(crate) use link::lower;

fn annotation(ann: Option<&ast::TsTypeAnn>) -> Result<Option<Type>> {
    let Some(ann) = ann else {
        return Ok(None);
    };
    let ast::TsType::TsKeywordType(keyword) = &*ann.type_ann else {
        return Err(unsupported("type annotation"));
    };
    Ok(Some(match keyword.kind {
        ast::TsKeywordTypeKind::TsNumberKeyword => Type::Number,
        ast::TsKeywordTypeKind::TsBooleanKeyword => Type::Boolean,
        ast::TsKeywordTypeKind::TsStringKeyword => Type::String,
        ast::TsKeywordTypeKind::TsVoidKeyword => Type::Void,
        _ => return Err(unsupported("type annotation")),
    }))
}
fn scalar(ty: Type) -> Result<()> {
    if matches!(ty, Type::Number | Type::Boolean) {
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
    body.iter().any(|s| match s {
        S::Return(_) => true,
        S::Block(b) => returns(b),
        S::If(_, a, b) => returns(a) && returns(b),
        _ => false,
    })
}

mod declarations;
mod expr;
mod link;
mod signature;
mod stmt;

impl Lower<'_> {
    fn declare(
        &mut self,
        name: &str,
        ty: Option<Type>,
        initialized: bool,
        mutable: bool,
    ) -> Result<()> {
        if matches!(name, "varta" | "console" | "log") || self.functions.contains_key(name) {
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
