//! V38 generator signature inference. The body is still checked by normal
//! lowering; this pass determines its yielded/returned types before callers.
use super::*;
use swc_ecma_visit::{Visit, VisitWith};

pub(super) fn infer(
    body: &ast::FunctionBody,
    bindings: impl IntoIterator<Item = (String, Type)>,
    types: &mut ir::ValueTypes,
) -> Result<Type> {
    let outer = types
        .type_level
        .inference_locals(bindings.into_iter().collect());
    let mut scan = Scan {
        types,
        yielded: Vec::new(),
        returned: Vec::new(),
        error: None,
        discarded: false,
    };
    body.visit_with(&mut scan);
    let result = (|| {
        if let Some(error) = scan.error.take() {
            Err(error)
        } else {
            let yielded = join(&scan.yielded, Type::Undefined, scan.types)?;
            let returned = if scan.returned.is_empty() {
                Type::Void
            } else {
                if !body.stmts.last().is_some_and(returns) {
                    scan.returned.push(Type::Undefined);
                }
                join(&scan.returned, Type::Void, scan.types)?
            };
            generators::intern(yielded, returned, Type::Undefined, scan.types)
        }
    })();
    types.type_level.restore_inference_locals(outer);
    result
}
fn join(values: &[Type], empty: Type, types: &mut ir::ValueTypes) -> Result<Type> {
    if values.is_empty() {
        return Ok(empty);
    }
    unions::intern(values.iter().map(|t| t.erased()).collect(), types)
}
fn returns(stmt: &ast::Stmt) -> bool {
    match stmt {
        ast::Stmt::Return(_) | ast::Stmt::Throw(_) => true,
        ast::Stmt::Block(b) => b.stmts.last().is_some_and(returns),
        ast::Stmt::If(i) => returns(&i.cons) && i.alt.as_deref().is_some_and(returns),
        _ => false,
    }
}
struct Scan<'a> {
    types: &'a mut ir::ValueTypes,
    yielded: Vec<Type>,
    returned: Vec<Type>,
    error: Option<CompilerError>,
    discarded: bool,
}
impl Scan<'_> {
    fn expression(&mut self, e: &ast::Expr) -> Option<Type> {
        match type_level::infer_expression(e, self.types) {
            Ok(t) => Some(t),
            Err(e) => {
                self.error.get_or_insert(e);
                None
            }
        }
    }
}
impl Visit for Scan<'_> {
    fn visit_function(&mut self, _: &ast::Function) {}
    fn visit_arrow_expr(&mut self, _: &ast::ArrowExpr) {}
    fn visit_class(&mut self, _: &ast::Class) {}
    fn visit_block_stmt(&mut self, block: &ast::BlockStmt) {
        let outer = self.types.type_level.inference_bindings();
        block.visit_children_with(self);
        self.types.type_level.restore_inference_locals(outer);
    }
    fn visit_expr(&mut self, expr: &ast::Expr) {
        let discarded = self.discarded;
        self.discarded = matches!(expr, ast::Expr::Paren(_)) && discarded;
        if let ast::Expr::Yield(y) = expr {
            if !discarded && !y.delegate {
                self.error.get_or_insert_with(|| type_error("generator inference requires an explicit Generator<T, TReturn, TNext> annotation when a yield result is consumed"));
            }
            let ty = y
                .arg
                .as_deref()
                .and_then(|e| self.expression(e))
                .unwrap_or(Type::Undefined);
            if y.delegate {
                match ty {
                    Type::Generator(id) => self
                        .yielded
                        .push(self.types.generators[id as usize].yielded),
                    _ => {
                        self.error.get_or_insert_with(|| {
                            type_error("generator inference requires a generator type for yield*")
                        });
                    }
                }
            } else {
                self.yielded.push(ty);
            }
        }
        expr.visit_children_with(self);
        self.discarded = discarded;
    }
    fn visit_expr_stmt(&mut self, stmt: &ast::ExprStmt) {
        self.discarded = true;
        stmt.expr.visit_with(self);
        self.discarded = false;
    }
    fn visit_for_of_stmt(&mut self, stmt: &ast::ForOfStmt) {
        let outer = self.types.type_level.inference_bindings();
        let element = self.expression(&stmt.right).and_then(|t| match t {
            Type::Array(id) => Some(self.types.arrays[id as usize]),
            Type::Generator(id) => Some(self.types.generators[id as usize].yielded),
            Type::String => Some(Type::String),
            _ => None,
        });
        if let (Some(ty), ast::ForHead::VarDecl(decl)) = (element, &stmt.left)
            && let Some(ast::VarDeclarator {
                name: ast::Pat::Ident(binding),
                ..
            }) = decl.decls.first()
        {
            self.types
                .type_level
                .inference_binding(binding.id.sym.to_string(), ty);
        }
        stmt.right.visit_with(self);
        stmt.body.visit_with(self);
        self.types.type_level.restore_inference_locals(outer);
    }
    fn visit_var_declarator(&mut self, decl: &ast::VarDeclarator) {
        if let ast::Pat::Ident(binding) = &decl.name {
            let result = annotation(binding.type_ann.as_deref(), self.types);
            let ty = match result {
                Ok(Some(t)) => Some(t),
                Ok(None) => decl.init.as_deref().and_then(|e| self.expression(e)),
                Err(e) => {
                    self.error.get_or_insert(e);
                    None
                }
            };
            if let Some(ty) = ty {
                self.types
                    .type_level
                    .inference_binding(binding.id.sym.to_string(), ty);
            }
        }
        decl.visit_children_with(self);
    }
    fn visit_return_stmt(&mut self, stmt: &ast::ReturnStmt) {
        let ty = stmt
            .arg
            .as_deref()
            .and_then(|e| self.expression(e))
            .unwrap_or(Type::Undefined);
        self.returned.push(ty);
        stmt.visit_children_with(self);
    }
}
