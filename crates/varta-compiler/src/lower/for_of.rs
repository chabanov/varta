//! Intrinsic bounded-array iteration, with live length and fresh lexical cells.
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn number(span: swc_common::Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn binary(span: swc_common::Span, op: ast::BinaryOp, left: Expr, right: Expr, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
impl Lower<'_> {
    pub(super) fn for_of(&mut self, statement: &ast::ForOfStmt) -> Result<S> {
        if !self.types.profile.closures() {
            return Err(unsupported("for-of requires checked-f64-utf16-v12"));
        }
        if statement.is_await {
            return Err(unsupported("for-await-of"));
        }
        self.scopes.push(BTreeMap::new());
        let (pattern, mode) = match &statement.left {
            ast::ForHead::VarDecl(declaration) => {
                if declaration.decls.len() != 1 || declaration.decls[0].init.is_some() {
                    return Err(self.source.locate(
                        unsupported("for-of declaration initializer or multiple bindings"),
                        declaration.span,
                    ));
                }
                self.predeclare(declaration)
                    .map_err(|e| self.source.locate(e, declaration.span))?;
                (&declaration.decls[0].name, patterns::Mode::Initialize)
            }
            ast::ForHead::Pat(pattern) => {
                let names =
                    patterns::targets(pattern).map_err(|(e, span)| self.source.locate(e, span))?;
                for (name, span) in names {
                    self.assignment_binding(&name, span)?;
                }
                (&**pattern, patterns::Mode::Assign)
            }
            _ => return Err(unsupported("for-of using declaration")),
        };
        // Lexical head names shadow outer names while evaluating the RHS and
        // remain in TDZ; assignment heads do not introduce bindings.
        let value = self.expr(&statement.right)?;
        let (source, array, mut roots, mut init) = self.for_of_source(value)?;
        let Type::Array(id) = array else {
            return Err(invalid("missing for-of array type"));
        };
        let element = self.types.arrays[id as usize];
        let item = if self.types.profile.sparse_arrays() {
            unions::intern(vec![element, Type::Undefined], self.types)?
        } else {
            element
        };
        let index = self.hidden(Type::Float64);
        let current = self.hidden(item);
        roots.push(index);
        init.push(S::Init(index, number(statement.span, 0.0)));
        let read = Expr {
            span: statement.right.span(),
            ty: item,
            kind: if self.types.profile.sparse_arrays() {
                E::MaybeIndex(
                    Box::new(local(statement.span, source, array)),
                    Box::new(local(statement.span, index, Type::Float64)),
                )
            } else {
                E::Index(
                    Box::new(local(statement.span, source, array)),
                    Box::new(local(statement.span, index, Type::Float64)),
                )
            },
        };
        // IteratorStepValue precedes evaluation of an assignment reference.
        // S::Init creates a new managed cell for every captured lexical leaf.
        let mut body = vec![S::Init(current, read)];
        body.extend(self.binding_pattern(
            pattern,
            local(pattern.span(), current, item),
            None,
            None,
            mode,
        )?);
        self.loops += 1;
        body.extend(self.single(&statement.body)?);
        self.loops -= 1;
        let slots = self
            .scopes
            .pop()
            .ok_or_else(|| invalid("missing for-of scope"))?
            .values()
            .map(|b| b.slot)
            .collect::<Vec<_>>();
        let mut iteration = vec![current];
        iteration.extend(slots);
        let test = binary(
            statement.span,
            ast::BinaryOp::Lt,
            local(statement.span, index, Type::Float64),
            Expr {
                span: statement.span,
                ty: Type::Float64,
                kind: E::Length(Box::new(local(statement.span, source, array))),
            },
            Type::Boolean,
        );
        let update = S::Set(
            index,
            binary(
                statement.span,
                ast::BinaryOp::Add,
                local(statement.span, index, Type::Float64),
                number(statement.span, 1.0),
                Type::Float64,
            ),
        );
        Ok(S::Scope(
            roots,
            vec![S::For(
                init,
                Some(test),
                vec![update],
                vec![S::Scope(iteration, body)],
            )],
        ))
    }
    fn for_of_source(&mut self, value: Expr) -> Result<(u32, Type, Vec<u32>, Vec<S>)> {
        let span = value.span;
        let members = unions::members(value.ty, self.types);
        let present: Vec<_> = members
            .iter()
            .copied()
            .filter(|ty| !matches!(ty, Type::Null | Type::Undefined))
            .collect();
        let array = match present.as_slice() {
            [ty @ Type::Array(_)] => *ty,
            _ => {
                return Err(self
                    .source
                    .locate(type_error("for-of requires a checked array"), span));
            }
        };
        let input = self.hidden(value.ty);
        let input_ty = value.ty;
        let mut roots = vec![input];
        let mut body = vec![S::Init(input, value)];
        if input_ty == array {
            return Ok((input, array, roots, body));
        }
        let error = self.array_error(
            varta_common::value::ErrorClass::TypeError,
            "Cannot iterate null or undefined",
            span,
        )?;
        body.push(S::If(
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::IsNullish(Box::new(local(span, input, input_ty))),
            },
            vec![S::Throw(error)],
            vec![],
        ));
        let source = self.hidden(array);
        roots.push(source);
        body.push(S::Init(
            source,
            Expr {
                span,
                ty: array,
                kind: E::UnboxUnion(Box::new(local(span, input, input_ty))),
            },
        ));
        Ok((source, array, roots, body))
    }
}
