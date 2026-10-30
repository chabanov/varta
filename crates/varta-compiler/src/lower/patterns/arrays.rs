use super::*;
impl Lower<'_> {
    fn pattern_done(
        &self,
        span: swc_common::Span,
        source: u32,
        ty: Type,
        index: usize,
        done: u32,
    ) -> S {
        S::If(
            binary(
                span,
                ast::BinaryOp::GtEq,
                number(span, index),
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Length(Box::new(local(span, source, ty))),
                },
                Type::Boolean,
            ),
            vec![S::Set(
                done,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(1),
                },
            )],
            vec![],
        )
    }
    pub(super) fn array_binding_pattern(
        &mut self,
        pattern: &ast::ArrayPat,
        value: Expr,
        mode: Mode,
    ) -> Result<Vec<S>> {
        let span = pattern.span;
        let value = self.match_array_values(value)?;
        let (source, ty, mut roots, mut body) = self.pattern_source(value, true)?;
        let Type::Array(id) = ty else {
            return Err(invalid("missing array pattern type"));
        };
        let element = self.types.arrays[id as usize];
        let read_type = unions::intern(vec![element, Type::Undefined], self.types)?;
        let done = self.hidden(Type::Boolean);
        roots.push(done);
        body.push(S::Init(
            done,
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Integer(0),
            },
        ));
        for (index, entry) in pattern.elems.iter().enumerate() {
            if let Some(ast::Pat::Rest(rest)) = entry {
                let prepared = if mode == Mode::Assign {
                    self.prepare_pattern_target(&rest.arg)?
                } else {
                    None
                };
                let (rest_value, rest_body, rest_roots) =
                    self.pattern_array_rest(span, source, ty, index, done)?;
                roots.extend(rest_roots);
                if let Some(target) = prepared {
                    body.extend(self.finish_pattern_target(target, rest_value, None, rest_body)?);
                } else {
                    body.extend(rest_body);
                    body.extend(self.binding_pattern(&rest.arg, rest_value, None, None, mode)?);
                }
                break;
            }
            let step = self.pattern_done(span, source, ty, index, done);
            if let Some(entry) = entry {
                let prepared = if mode == Mode::Assign {
                    self.prepare_pattern_target(entry)?
                } else {
                    None
                };
                let absent = unions::widen(undefined(span), Some(read_type), self.types)?;
                let read = Expr {
                    span,
                    ty: read_type,
                    kind: E::MaybeIndex(
                        Box::new(local(span, source, ty)),
                        Box::new(number(span, index)),
                    ),
                };
                let value = Expr {
                    span,
                    ty: read_type,
                    kind: E::Conditional(
                        Box::new(local(span, done, Type::Boolean)),
                        Box::new(absent),
                        Box::new(read),
                    ),
                };
                if let Some(target) = prepared {
                    let default = if let ast::Pat::Assign(assign) = entry {
                        Some(&*assign.right)
                    } else {
                        None
                    };
                    body.extend(self.finish_pattern_target(target, value, default, vec![step])?);
                } else {
                    body.push(step);
                    body.extend(self.binding_pattern(entry, value, None, None, mode)?);
                }
            } else {
                body.push(step);
            }
        }
        Ok(vec![S::Scope(roots, body)])
    }
    fn pattern_array_rest(
        &mut self,
        span: swc_common::Span,
        source: u32,
        ty: Type,
        start: usize,
        done: u32,
    ) -> Result<(Expr, Vec<S>, Vec<u32>)> {
        let Type::Array(id) = ty else {
            return Err(invalid("missing rest source"));
        };
        let element = self.types.arrays[id as usize];
        let element = if self.types.profile.sparse_arrays() {
            unions::intern(vec![element, Type::Undefined], self.types)?
        } else {
            element
        };
        let output_ty = super::super::arrays::intern(element, self.types)?;
        let output = self.hidden(output_ty);
        let index = self.hidden(Type::Float64);
        let length = self.hidden(Type::Float64);
        let limit = self.hidden(Type::Float64);
        let mut body = vec![
            S::Init(
                limit,
                Expr {
                    span,
                    ty: Type::Float64,
                    kind: E::Length(Box::new(local(span, source, ty))),
                },
            ),
            S::Init(
                length,
                binary(
                    span,
                    ast::BinaryOp::Sub,
                    local(span, limit, Type::Float64),
                    number(span, start),
                    Type::Float64,
                ),
            ),
        ];
        body.push(S::If(
            binary(
                span,
                ast::BinaryOp::LogicalOr,
                local(span, done, Type::Boolean),
                binary(
                    span,
                    ast::BinaryOp::Lt,
                    local(span, length, Type::Float64),
                    number(span, 0),
                    Type::Boolean,
                ),
                Type::Boolean,
            ),
            vec![S::Set(length, number(span, 0))],
            vec![],
        ));
        let kind = if self.types.profile.sparse_arrays() {
            E::ArrayReserve {
                length: Box::new(local(span, length, Type::Float64)),
                count: self.hidden(Type::Number),
                bytes: self.hidden(Type::Number),
            }
        } else {
            E::Array(vec![])
        };
        body.push(S::Init(
            output,
            Expr {
                span,
                ty: output_ty,
                kind,
            },
        ));
        let source_index = binary(
            span,
            ast::BinaryOp::Add,
            number(span, start),
            local(span, index, Type::Float64),
            Type::Float64,
        );
        let value = Expr {
            span,
            ty: element,
            kind: if self.types.profile.sparse_arrays() {
                E::MaybeIndex(Box::new(local(span, source, ty)), Box::new(source_index))
            } else {
                E::Index(Box::new(local(span, source, ty)), Box::new(source_index))
            },
        };
        let append = if self.types.profile.sparse_arrays() {
            S::SetIndex {
                array: local(span, output, output_ty),
                index: local(span, index, Type::Float64),
                value,
                slots: Some((
                    self.hidden(output_ty),
                    self.hidden(if self.types.profile.array_mutations() {
                        Type::Float64
                    } else {
                        Type::Number
                    }),
                )),
            }
        } else {
            S::Expr(Expr {
                span,
                ty: Type::Float64,
                kind: E::ArrayPush(
                    Box::new(local(span, output, output_ty)),
                    Box::new(Expr {
                        span,
                        ty: output_ty,
                        kind: E::Array(vec![Some(value)]),
                    }),
                ),
            })
        };
        body.push(S::For(
            vec![S::Init(index, number(span, 0))],
            Some(binary(
                span,
                ast::BinaryOp::Lt,
                local(span, index, Type::Float64),
                local(span, length, Type::Float64),
                Type::Boolean,
            )),
            vec![S::Set(
                index,
                binary(
                    span,
                    ast::BinaryOp::Add,
                    local(span, index, Type::Float64),
                    number(span, 1),
                    Type::Float64,
                ),
            )],
            vec![append],
        ));
        Ok((local(span, output, output_ty), body, vec![output]))
    }
}
