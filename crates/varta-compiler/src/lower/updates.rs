//! Checked Number updates return old/new values only after a successful store.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
pub(super) fn next(span: swc_common::Span, input: u32, op: ast::UpdateOp) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Binary(
            match op {
                ast::UpdateOp::PlusPlus => ast::BinaryOp::Add,
                ast::UpdateOp::MinusMinus => ast::BinaryOp::Sub,
            },
            Box::new(local(span, input, Type::Float64)),
            Box::new(Expr {
                span,
                ty: Type::Float64,
                kind: E::Float64(1.0),
            }),
        ),
    }
}
impl Lower<'_> {
    pub(super) fn update_value(&mut self, update: &ast::UpdateExpr) -> Result<Expr> {
        if !self.types.profile.closures() {
            return Err(unsupported(
                "increment/decrement require checked-f64-utf16-v12",
            ));
        }
        let target = match stmt::unparen(&update.arg) {
            ast::Expr::Ident(name) => self
                .prepare_pattern_target(&ast::Pat::Ident(ast::BindingIdent {
                    id: name.clone(),
                    type_ann: None,
                }))?
                .ok_or_else(|| invalid("missing update binding"))?,
            ast::Expr::Member(member) => {
                let base = self.expr(&member.obj)?;
                if let Type::Record(shape) = base.ty
                    && let ast::MemberProp::Computed(key) = &member.prop
                    && !matches!(
                        &*key.expr,
                        ast::Expr::Lit(ast::Lit::Str(_) | ast::Lit::Num(_))
                    )
                {
                    return self.dynamic_record_update_value(update, base, &key.expr, shape);
                }
                let field = self.place_value(member, base)?;
                self.prepare_member_reference(field, member.span)?
            }
            _ => return Err(unsupported("increment/decrement target")),
        };
        self.prepared_update_value(target, update)
    }
    fn dynamic_record_update_value(
        &mut self,
        update: &ast::UpdateExpr,
        base: Expr,
        key: &ast::Expr,
        shape: u32,
    ) -> Result<Expr> {
        let key = self.expr(key)?;
        if key.ty != Type::String {
            return Err(self.source.locate(
                type_error(
                    "computed record key must be a narrowed string; coercion is unsupported",
                ),
                key.span,
            ));
        }
        let span = update.span;
        let record = Type::Record(shape);
        let source = self.hidden(record);
        let name = self.hidden(Type::String);
        let read = self.dynamic_record_read(
            update.arg.span(),
            local(span, source, record),
            local(span, name, Type::String),
            shape,
        )?;
        let old = self.checked_update_number(read)?;
        let input = self.hidden(Type::Float64);
        let output = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        let store = self.checked_record_write_value(
            span,
            local(span, source, record),
            local(span, name, Type::String),
            next(span, input, update.op),
            shape,
        )?;
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::ValueBlock {
                output: if update.prefix { output } else { input },
                marker,
                roots: vec![source, name, input, output],
                body: vec![
                    S::Init(source, base),
                    S::Init(name, key),
                    S::Init(input, old),
                    S::Init(output, store),
                ],
            },
        })
    }
    pub(super) fn checked_update_number(&mut self, value: Expr) -> Result<Expr> {
        if !unions::members(value.ty, self.types).contains(&Type::Float64) {
            return Err(self.source.locate(
                type_error("increment/decrement require a Number target; coercion is unsupported"),
                value.span,
            ));
        }
        if value.ty == Type::Float64 {
            return Ok(value);
        }
        let span = value.span;
        let ty = value.ty;
        let input = self.hidden(ty);
        let output = self.hidden(Type::Float64);
        let marker = self.hidden(Type::Number);
        let number = Expr {
            span,
            ty: Type::Float64,
            kind: E::UnboxUnion(Box::new(local(span, input, ty))),
        };
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsUnionMember(Box::new(local(span, input, ty)), Type::Float64),
        };
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Invalid update operand",
            span,
        )?;
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![input, output],
                body: vec![
                    S::Init(input, value),
                    S::If(test, vec![S::Init(output, number)], vec![S::Throw(error)]),
                ],
            },
        })
    }
}
