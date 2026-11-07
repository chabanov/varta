//! V38 K9 C expression forms (`expression_forms`): the comma operator and
//! `new.target` comparisons. Each form is recognized syntactically by
//! [`handles`] before the ordinary dispatch, so earlier profiles keep their
//! diagnostics and bytes.
use super::*;

/// Whether the K9 C lowering owns this expression form.
pub(super) fn handles(expression: &ast::Expr) -> bool {
    match expression {
        ast::Expr::Seq(_) => true,
        ast::Expr::Bin(binary) => new_target_comparison(binary).is_some(),
        ast::Expr::TaggedTpl(_) => true,
        _ => false,
    }
}

/// The operand compared with `new.target`, if `binary` is such a comparison.
pub(super) fn new_target_operand(binary: &ast::BinExpr) -> Option<&ast::Expr> {
    new_target_comparison(binary).map(|(other, _)| other)
}

/// `new.target === X` or `new.target !== X` (either operand order): the
/// other operand and whether the comparison is negated.
fn new_target_comparison(binary: &ast::BinExpr) -> Option<(&ast::Expr, bool)> {
    let negated = match binary.op {
        ast::BinaryOp::EqEqEq | ast::BinaryOp::EqEq => false,
        ast::BinaryOp::NotEqEq | ast::BinaryOp::NotEq => true,
        _ => return None,
    };
    let is_target = |e: &ast::Expr| matches!(stmt::unparen(e), ast::Expr::MetaProp(meta) if meta.kind == ast::MetaPropKind::NewTarget);
    if is_target(&binary.left) {
        Some((&*binary.right, negated))
    } else if is_target(&binary.right) {
        Some((&*binary.left, negated))
    } else {
        None
    }
}

impl Lower<'_> {
    pub(super) fn expression_form(&mut self, expression: &ast::Expr) -> Result<Expr> {
        match expression {
            ast::Expr::Seq(sequence) => self.sequence(sequence),
            ast::Expr::Bin(binary) => match new_target_comparison(binary) {
                Some((other, negated)) => self.new_target_equals(binary.span, other, negated),
                None => Err(CompilerError::Internal(
                    "missing new.target comparison".into(),
                )),
            },
            ast::Expr::TaggedTpl(tagged) => self.tagged_template(tagged),
            _ => Err(CompilerError::Internal("unclaimed expression form".into())),
        }
    }

    /// `a, b, c`: every operand in source order; the value is the last one.
    fn sequence(&mut self, sequence: &ast::SeqExpr) -> Result<Expr> {
        let mut values = sequence.exprs.iter();
        let first = values
            .next()
            .ok_or_else(|| CompilerError::Internal("empty sequence expression".into()))?;
        let mut result = self.expr(first)?;
        for value in values {
            let next = self.expr(value)?;
            result = Expr {
                span: sequence.span,
                ty: next.ty,
                kind: E::Sequence(Box::new(result), Box::new(next)),
            };
        }
        Ok(result)
    }

    /// `new.target === C` in a class constructor tests the class of the
    /// instance under construction exactly (the most-derived class `new`
    /// named); `new.target === undefined` is false there. Outside a
    /// constructor a checked program never calls a function with `new`, so
    /// `new.target` is `undefined`.
    fn new_target_equals(
        &mut self,
        span: swc_common::Span,
        other: &ast::Expr,
        negated: bool,
    ) -> Result<Expr> {
        let boolean = |value: bool| Expr {
            span,
            ty: Type::Boolean,
            kind: E::Integer(i32::from(value != negated)),
        };
        let undefined = matches!(stmt::unparen(other), ast::Expr::Ident(id) if id.sym == "undefined" && self.binding("undefined").is_err());
        let context = self
            .types
            .classes
            .context
            .filter(|c| c.init == Some(self.index));
        let Some(context) = context else {
            if undefined {
                return Ok(boolean(true));
            }
            return Err(self.source.locate(
                unsupported("new.target outside a class constructor compared with a value other than undefined"),
                span,
            ));
        };
        if undefined {
            return Ok(boolean(false));
        }
        let Some(class) = self.class_callee(other) else {
            return Err(self.source.locate(
                unsupported("new.target compared with a value that is not a class"),
                other.span(),
            ));
        };
        let classes = &self.types.classes;
        if class != context.class && !classes.descends(class, context.class) {
            // A class outside the constructed family is never `new.target`.
            return Ok(boolean(false));
        }
        let shape = classes.shape(class);
        let test = Expr {
            span,
            ty: Type::Boolean,
            kind: E::IsClass {
                value: Box::new(self.expr(&ast::Expr::This(ast::ThisExpr { span }))?),
                shapes: vec![shape],
                slot: self.hidden(Type::Number),
            },
        };
        Ok(if negated {
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Not(Box::new(test)),
            }
        } else {
            test
        })
    }
}

impl Lower<'_> {
    /// `o.m(args)` where field `m` holds a function with a `this` parameter:
    /// the object is the receiver. The base is a binding or `this`, read
    /// again for the receiver (a read without effects); other bases are
    /// refused.
    pub(super) fn receiver_field_call(
        &mut self,
        call: &ast::CallExpr,
        callee: &ast::Expr,
        value: Expr,
    ) -> Result<Expr> {
        let ast::Expr::Member(member) = stmt::unparen(callee) else {
            return Err(CompilerError::Internal(
                "missing receiver field call".into(),
            ));
        };
        if !binding_path(&member.obj) {
            return Err(self.source.locate(
                unsupported("method-style call of a function field with a this parameter on a base other than a binding or this"),
                member.obj.span(),
            ));
        }
        let receiver = self.expr(&member.obj)?;
        let Type::Function(index) = value.ty else {
            return Err(CompilerError::Internal("missing receiver function".into()));
        };
        let signature = self.types.functions[index as usize].clone();
        if !signature.shape.extended() && call.args.len() != signature.params.len() {
            return Err(invalid("callback argument count mismatch"));
        }
        let args = self.call_arguments(&call.args, &signature.params, &signature.shape)?;
        self.invoke_receiver(value, Some(receiver), args, call.span)
    }
}

/// An identifier or `this`: reading it again has no effect.
pub(super) fn binding_path(expr: &ast::Expr) -> bool {
    matches!(
        stmt::unparen(expr),
        ast::Expr::Ident(_) | ast::Expr::This(_)
    )
}
