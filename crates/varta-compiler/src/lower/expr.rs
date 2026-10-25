use super::*;

impl Lower<'_> {
    pub(super) fn condition(&mut self, expr: &ast::Expr) -> Result<Expr> {
        let expr = self.expr(expr)?;
        scalar(expr.ty).map_err(|e| self.source.locate(e, expr.span))?;
        Ok(expr)
    }
    pub(super) fn expr(&mut self, expression: &ast::Expr) -> Result<Expr> {
        self.depth += 1;
        if self.depth > 64 {
            return Err(self
                .source
                .locate(invalid("expression nesting exceeds 64"), expression.span()));
        }
        let result = self.expr_inner(expression);
        self.depth -= 1;
        result.map_err(|e| self.source.locate(e, expression.span()))
    }
    fn expr_inner(&mut self, expression: &ast::Expr) -> Result<Expr> {
        let (ty, kind) = match expression {
            ast::Expr::Lit(ast::Lit::Num(n)) => {
                if n.value.fract() != 0.0 || n.value < i32::MIN as f64 || n.value > i32::MAX as f64
                {
                    return Err(invalid("only integer literals in i32 range are supported"));
                }
                (Type::Number, E::Integer(n.value as i32))
            }
            ast::Expr::Lit(ast::Lit::Bool(b)) => (Type::Boolean, E::Integer(i32::from(b.value))),
            ast::Expr::Lit(ast::Lit::Str(s)) => (
                Type::String,
                E::String(
                    s.value
                        .as_str()
                        .ok_or_else(|| unsupported("non-UTF-8 string"))?
                        .into(),
                ),
            ),
            ast::Expr::Ident(name) => {
                let binding = self.binding(name.sym.as_ref())?;
                if !binding.initialized {
                    return Err(invalid(format!(
                        "binding used before initialization: {}",
                        name.sym
                    )));
                }
                (
                    binding.ty.ok_or_else(|| invalid("missing binding type"))?,
                    E::Local(binding.slot),
                )
            }
            ast::Expr::Paren(paren) => return self.expr(&paren.expr),
            ast::Expr::Unary(unary) => {
                let value = self.expr(&unary.arg)?;
                match unary.op {
                    ast::UnaryOp::Minus if value.ty == Type::Number => {
                        (Type::Number, E::Negative(Box::new(value)))
                    }
                    ast::UnaryOp::Bang if matches!(value.ty, Type::Number | Type::Boolean) => {
                        (Type::Boolean, E::Not(Box::new(value)))
                    }
                    _ => return Err(unsupported("unary operator")),
                }
            }
            ast::Expr::Bin(binary) => {
                let left = self.expr(&binary.left)?;
                let right = self.expr(&binary.right)?;
                scalar(left.ty)?;
                if left.ty != right.ty {
                    return Err(self.source.locate(
                        type_error(format!(
                            "binary operand type mismatch: expected {}, found {}",
                            left.ty, right.ty
                        )),
                        right.span,
                    ));
                }
                let result = match binary.op {
                    ast::BinaryOp::Add
                    | ast::BinaryOp::Sub
                    | ast::BinaryOp::Mul
                    | ast::BinaryOp::Div
                    | ast::BinaryOp::Mod
                        if left.ty == Type::Number =>
                    {
                        Type::Number
                    }
                    ast::BinaryOp::Lt
                    | ast::BinaryOp::LtEq
                    | ast::BinaryOp::Gt
                    | ast::BinaryOp::GtEq
                        if left.ty == Type::Number =>
                    {
                        Type::Boolean
                    }
                    ast::BinaryOp::EqEq
                    | ast::BinaryOp::EqEqEq
                    | ast::BinaryOp::NotEq
                    | ast::BinaryOp::NotEqEq => Type::Boolean,
                    ast::BinaryOp::LogicalAnd | ast::BinaryOp::LogicalOr => left.ty,
                    _ => return Err(unsupported("binary operator")),
                };
                (
                    result,
                    E::Binary(binary.op, Box::new(left), Box::new(right)),
                )
            }
            ast::Expr::Call(call) => return self.call(call),
            _ => return Err(unsupported("expression")),
        };
        Ok(Expr {
            span: expression.span(),
            ty,
            kind,
        })
    }
    fn call(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        if call.type_args.is_some() || call.args.iter().any(|a| a.spread.is_some()) {
            return Err(unsupported("generic or spread call"));
        }
        let ast::Callee::Expr(callee) = &call.callee else {
            return Err(unsupported("callee"));
        };
        let name = callee_name(callee).ok_or_else(|| unsupported("callee"))?;
        if let Some(signature) = self.functions.get(&name).cloned() {
            if name == "main" {
                return Err(unsupported("calling main"));
            }
            if call.args.len() != signature.params.len() {
                return Err(invalid(format!(
                    "function {name} expects {} arguments",
                    signature.params.len()
                )));
            }
            let mut args = Vec::new();
            for (arg, ty) in call.args.iter().zip(signature.params) {
                let value = self.expr(&arg.expr)?;
                if value.ty != ty {
                    return Err(self.source.locate(
                        type_error(format!(
                            "argument type mismatch: expected {ty}, found {}",
                            value.ty
                        )),
                        value.span,
                    ));
                }
                args.push(value);
            }
            return Ok(Expr {
                span: call.span,
                ty: signature.result,
                kind: E::Call(signature.index, args),
            });
        }
        if name == "varta.result.status" {
            if !call.args.is_empty() {
                return Err(invalid("varta.result.status expects no arguments"));
            }
            return Ok(Expr {
                span: call.span,
                ty: Type::Number,
                kind: E::Status,
            });
        }
        let (host, expected, result) = match name.as_str() {
            "console.log" | "log" => {
                if call.args.len() != 1 {
                    return Err(invalid("log expects one argument"));
                }
                let arg = self.expr(&call.args[0].expr)?;
                let host = match arg.ty {
                    Type::String => Host::LogString,
                    Type::Number | Type::Boolean => Host::LogInt,
                    _ => return Err(self.source.locate(type_error("cannot log void"), arg.span)),
                };
                return Ok(Expr {
                    span: call.span,
                    ty: Type::Void,
                    kind: E::Host(host, vec![arg]),
                });
            }
            "varta.time.now" => (Host::Time, None, Type::Number),
            "varta.env.has" => (Host::EnvHas, Some(Type::String), Type::Boolean),
            "varta.env.get" => (Host::EnvGet, Some(Type::String), Type::String),
            "varta.env.print" => (Host::EnvPrint, Some(Type::String), Type::Void),
            "varta.fs.read" => (Host::FsRead, Some(Type::String), Type::String),
            "varta.fs.print" => (Host::FsPrint, Some(Type::String), Type::Void),
            "varta.net.fetch" => (Host::NetFetch, Some(Type::String), Type::Number),
            "varta.net.get" => (Host::NetGet, Some(Type::String), Type::String),
            "varta.process.exit" => (Host::Exit, Some(Type::Number), Type::Void),
            _ => return Err(unsupported("call")),
        };
        if call.args.len() != usize::from(expected.is_some()) {
            return Err(invalid(format!("invalid argument count for {name}")));
        }
        let mut args = Vec::new();
        for arg in &call.args {
            let value = self.expr(&arg.expr)?;
            if Some(value.ty) != expected {
                return Err(self.source.locate(
                    type_error(format!(
                        "argument type mismatch for {name}: expected {}, found {}",
                        expected.unwrap_or(Type::Void),
                        value.ty
                    )),
                    value.span,
                ));
            }
            if value.ty == Type::String && !matches!(value.kind, E::String(_)) {
                return Err(unsupported("dynamic host string argument"));
            }
            args.push(value);
        }
        Ok(Expr {
            span: call.span,
            ty: result,
            kind: E::Host(host, args),
        })
    }
}

fn callee_name(expr: &ast::Expr) -> Option<String> {
    match expr {
        ast::Expr::Ident(name) => Some(name.sym.to_string()),
        ast::Expr::Member(member) => {
            let ast::MemberProp::Ident(name) = &member.prop else {
                return None;
            };
            Some(format!("{}.{}", callee_name(&member.obj)?, name.sym))
        }
        _ => None,
    }
}
