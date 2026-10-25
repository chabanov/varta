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
    result: Type,
    loops: usize,
    switches: usize,
    depth: usize,
}

pub(crate) fn lower(module: &ast::Module, source: &Source) -> Result<ir::Program> {
    let mut declarations = Vec::new();
    for item in &module.body {
        let declaration = match item {
            ast::ModuleItem::Stmt(ast::Stmt::Decl(decl)) => decl,
            ast::ModuleItem::ModuleDecl(ast::ModuleDecl::ExportDecl(export)) => &export.decl,
            ast::ModuleItem::Stmt(ast::Stmt::Empty(_)) => continue,
            _ => {
                return Err(source.locate(
                    unsupported("top-level statement or module import"),
                    item.span(),
                ));
            }
        };
        match declaration {
            ast::Decl::Fn(function) if !function.declare => declarations.push(function),
            ast::Decl::TsInterface(_) | ast::Decl::TsTypeAlias(_) => {}
            ast::Decl::Var(var) if var.declare => {
                for declaration in &var.decls {
                    let ast::Pat::Ident(binding) = &declaration.name else {
                        return Err(
                            source.locate(unsupported("ambient declaration"), declaration.span())
                        );
                    };
                    if !matches!(binding.id.sym.as_ref(), "varta" | "console" | "log")
                        || declaration.init.is_some()
                    {
                        return Err(
                            source.locate(unsupported("ambient declaration"), declaration.span())
                        );
                    }
                }
            }
            _ => {
                return Err(source.locate(unsupported("top-level declaration"), declaration.span()));
            }
        }
    }
    if declarations.len() > 256 {
        return Err(invalid("too many functions"));
    }
    let mut signatures = BTreeMap::new();
    let mut main = None;
    for (index, declaration) in declarations.iter().enumerate() {
        let name = declaration.ident.sym.to_string();
        let signature = signature::check(declaration, index, source)
            .map_err(|e| source.locate(e, declaration.ident.span))?;
        if name == "main" {
            if !signature.params.is_empty() || signature.result != Type::Void {
                return Err(source.locate(
                    invalid("main must have signature () -> void"),
                    declaration.ident.span,
                ));
            }
            main = Some(index);
        }
        if matches!(name.as_str(), "varta" | "console" | "log")
            || signatures.insert(name.clone(), signature).is_some()
        {
            return Err(source.locate(
                invalid(format!("duplicate or reserved function name: {name}")),
                declaration.ident.span,
            ));
        }
    }
    let main = main.ok_or_else(|| invalid("expected a top-level function named main()"))?;
    let mut functions = Vec::new();
    for declaration in declarations {
        let signature = &signatures[declaration.ident.sym.as_ref()];
        let mut lower = Lower {
            source,
            functions: &signatures,
            scopes: vec![BTreeMap::new()],
            next: 0,
            result: signature.result,
            loops: 0,
            switches: 0,
            depth: 0,
        };
        for (param, ty) in declaration.function.params.iter().zip(&signature.params) {
            let ast::Pat::Ident(binding) = &param.pat else {
                return Err(unsupported("parameter pattern"));
            };
            lower
                .declare(binding.id.sym.as_ref(), Some(*ty), true, true)
                .map_err(|e| source.locate(e, binding.span()))?;
        }
        // Bodies were required while collecting signatures.
        let body = declaration
            .function
            .body
            .as_ref()
            .ok_or_else(|| invalid("missing function body"))?;
        let statements = lower.block(&body.stmts, false)?;
        if signature.result != Type::Void && !returns(&statements) {
            return Err(source.locate(
                invalid("value-returning function must return on every path"),
                declaration.ident.span,
            ));
        }
        functions.push(ir::Function {
            span: declaration.span(),
            params: signature.params.clone(),
            result: signature.result,
            locals: lower.next,
            body: statements,
        });
    }
    Ok(ir::Program { functions, main })
}

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
        Err(unsupported("non-scalar local or parameter"))
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

mod expr;
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
        Ok(())
    }
}
