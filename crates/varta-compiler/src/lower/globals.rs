//! Module variables (`let`/`var` at module level, v37 `language_core`).
//!
//! A module variable lives in a permanent root slot for the program lifetime,
//! like a stored module constant; a primitive is boxed in a union cell so the
//! slot always holds a traced handle. Unlike constants, functions never copy
//! it into a local: every read loads the slot and every write stores it, so
//! all functions, closures and importing modules observe the current value
//! (exports are live bindings). Importing modules may not assign it. Writes
//! are simple, compound, logical and update assignments of the identifier;
//! destructuring targets are rejected. A call or any other reentry may change
//! the variable, so guard facts on it are invalidated like those of mutable
//! captured bindings.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

impl Lower<'_> {
    /// The current value of module variable `binding`, narrowed by this
    /// function's facts on it.
    pub(super) fn global_read(
        &mut self,
        binding: &Binding,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let (index, storage) = binding
            .global
            .ok_or_else(|| invalid("missing module variable"))?;
        let ty = binding.ty.ok_or_else(|| invalid("missing binding type"))?;
        let stored = Expr {
            span,
            ty: storage,
            kind: E::ModuleConstant(index),
        };
        let value = if storage == ty {
            stored
        } else {
            Expr {
                span,
                ty,
                kind: E::UnboxUnion(Box::new(stored)),
            }
        };
        let narrow = (binding.identity.0 == self.index)
            .then(|| self.refinements.get(&binding.slot).copied())
            .flatten();
        Ok(match narrow {
            Some(narrow) if narrow != ty && ty.is_cell() => {
                if narrow.is_cell() {
                    Expr {
                        ty: narrow,
                        ..value
                    }
                } else {
                    Expr {
                        span,
                        ty: narrow,
                        kind: E::UnboxUnion(Box::new(value)),
                    }
                }
            }
            _ => value,
        })
    }
    /// Store `value` (of the variable's declared type) into its slot.
    pub(super) fn global_store(&mut self, binding: &Binding, value: Expr) -> Result<S> {
        let (index, storage) = binding
            .global
            .ok_or_else(|| invalid("missing module variable"))?;
        if !binding.mutable {
            return Err(invalid("assignment to an imported module variable"));
        }
        let ty = binding.ty.ok_or_else(|| invalid("missing binding type"))?;
        let value = unions::widen(value, Some(ty), self.types)?;
        self.flow_assigned(binding.slot, value.ty)?;
        Ok(S::InitModuleConstant(
            index,
            unions::widen(value, Some(storage), self.types)?,
        ))
    }
    /// `x = value` as an expression: stores, then evaluates to the value.
    pub(super) fn global_assignment_value(
        &mut self,
        binding: &Binding,
        right: &ast::Expr,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let ty = binding.ty.ok_or_else(|| invalid("missing binding type"))?;
        let value = self.expr_expected(right, Some(ty))?;
        if !arguments::accepts(ty, value.ty, self.types) {
            return Err(type_error(format!(
                "assignment type mismatch: expected {ty}, found {}",
                value.ty
            )));
        }
        let value = unions::widen(value, Some(ty), self.types)?;
        let saved = self.hidden(ty);
        let store = self.global_store(binding, local(span, saved, ty))?;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![saved],
                body: vec![
                    S::Init(saved, value),
                    store,
                    S::Init(output, local(span, saved, ty)),
                ],
            },
        })
    }
    /// The module variable named by `target`, when it is one.
    pub(super) fn global_target(&mut self, target: &ast::Expr) -> Option<Binding> {
        let ast::Expr::Ident(id) = stmt::unparen(target) else {
            return None;
        };
        self.resolve_binding(id.sym.as_ref())
            .ok()
            .filter(|binding| binding.global.is_some())
    }
    /// `x op= e`, `x &&= e`, `x ||= e` and `x ??= e` on a module variable,
    /// through their plain-assignment forms (`x` is read once per operand).
    pub(super) fn global_compound(&mut self, assign: &ast::AssignExpr) -> Result<Option<Expr>> {
        let ast::AssignTarget::Simple(ast::SimpleAssignTarget::Ident(name)) = &assign.left else {
            return Ok(None);
        };
        let target = ast::Expr::Ident(name.id.clone());
        let Some(binding) = self.global_target(&target) else {
            return Ok(None);
        };
        let span = assign.span;
        let operator = match assign.op {
            ast::AssignOp::Assign => {
                return self
                    .global_assignment_value(&binding, &assign.right, span)
                    .map(Some);
            }
            ast::AssignOp::AndAssign => Some(ast::BinaryOp::LogicalAnd),
            ast::AssignOp::OrAssign => Some(ast::BinaryOp::LogicalOr),
            ast::AssignOp::NullishAssign => Some(ast::BinaryOp::NullishCoalescing),
            _ => None,
        };
        let plain = |right: Box<ast::Expr>| {
            ast::Expr::Assign(ast::AssignExpr {
                span,
                op: ast::AssignOp::Assign,
                left: assign.left.clone(),
                right,
            })
        };
        let rewritten = if let Some(op) = operator {
            ast::Expr::Bin(ast::BinExpr {
                span,
                op,
                left: Box::new(target),
                right: Box::new(ast::Expr::Paren(ast::ParenExpr {
                    span,
                    expr: Box::new(plain(assign.right.clone())),
                })),
            })
        } else {
            let op = assign
                .op
                .to_update()
                .ok_or_else(|| unsupported("module variable compound assignment"))?;
            plain(Box::new(ast::Expr::Bin(ast::BinExpr {
                span,
                op,
                left: Box::new(target),
                right: Box::new(ast::Expr::Paren(ast::ParenExpr {
                    span,
                    expr: assign.right.clone(),
                })),
            })))
        };
        self.expr(&rewritten).map(Some)
    }
    /// `x++`, `++x`, `x--` and `--x` on a numeric module variable.
    pub(super) fn global_update(&mut self, update: &ast::UpdateExpr) -> Result<Option<Expr>> {
        let Some(binding) = self.global_target(&update.arg) else {
            return Ok(None);
        };
        let span = update.span;
        let ty = binding.ty.ok_or_else(|| invalid("missing binding type"))?;
        if ty != Type::Float64 {
            return Err(unsupported(
                "increment or decrement of a non-number module variable",
            ));
        }
        let old = self.hidden(ty);
        let new = self.hidden(ty);
        let read = self.global_read(&binding, span)?;
        let op = match update.op {
            ast::UpdateOp::PlusPlus => ast::BinaryOp::Add,
            ast::UpdateOp::MinusMinus => ast::BinaryOp::Sub,
        };
        let next = Expr {
            span,
            ty,
            kind: E::Binary(
                op,
                Box::new(local(span, old, ty)),
                Box::new(Expr {
                    span,
                    ty,
                    kind: E::Float64(1.0),
                }),
            ),
        };
        let store = self.global_store(&binding, local(span, new, ty))?;
        let output = self.hidden(ty);
        let marker = self.hidden(Type::Number);
        let result = if update.prefix { new } else { old };
        Ok(Some(Expr {
            span,
            ty,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![],
                body: vec![
                    S::Init(old, read),
                    S::Init(new, next),
                    store,
                    S::Init(output, local(span, result, ty)),
                ],
            },
        }))
    }
}
