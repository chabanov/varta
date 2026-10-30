//! `Array.isArray` narrowing of `unknown` to `unknown[]` (v38
//! `structural_subtyping`).
//!
//! An array in an `unknown` cell keeps its own element kind: a JSON graph
//! array holds `unknown` cells, a `number[]` holds raw numbers. A read of a
//! binding narrowed by `Array.isArray` calls one synthesized function that
//! returns the array itself when its elements are cells, and otherwise a
//! fresh `unknown[]` with every element boxed by its kind (a snapshot: writes
//! through it do not reach the original array). Arrays of function or
//! promise values throw a `TypeError`.
use super::*;
use varta_common::value::{ErrorClass, ValueKind};

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

impl Lower<'_> {
    /// `unknown[]`, the type `Array.isArray` proves for an `unknown` value.
    pub(super) fn unknown_array_type(types: &mut ir::ValueTypes) -> Result<Type> {
        arrays::intern(Type::Unknown, types)
    }
    /// The `unknown[]` view of the array in the cell `value`.
    pub(super) fn unknown_array(&mut self, value: Expr) -> Result<Expr> {
        let span = value.span;
        let any = Self::unknown_array_type(self.types)?;
        let index = match self.types.unknown_array {
            Some(index) => index,
            None => {
                let index = self.reserve_function("too many functions across modules")?;
                self.types.unknown_array = Some(index);
                self.define_function(index, span, vec![Type::Unknown], any, |child, args| {
                    child.unknown_array_body(span, any, args)
                })?;
                index
            }
        };
        Ok(Expr {
            span,
            ty: any,
            kind: E::Call(index, vec![value]),
        })
    }
    fn unknown_array_body(
        &mut self,
        span: swc_common::Span,
        any: Type,
        args: Vec<Expr>,
    ) -> Result<Vec<S>> {
        let cell = args
            .into_iter()
            .next()
            .ok_or_else(|| CompilerError::Internal("unknown array argument".into()))?;
        let number = self.types.number();
        let array = self.hidden(any);
        let out = self.hidden(any);
        let position = self.hidden(number);
        let kind_test = |kind: ValueKind| Expr {
            span,
            ty: Type::Boolean,
            kind: E::ArrayKind(Box::new(local(span, array, any)), kind),
        };
        let mut body = vec![
            S::Init(
                array,
                Expr {
                    span,
                    ty: any,
                    kind: E::UnboxUnion(Box::new(cell)),
                },
            ),
            S::If(
                kind_test(ValueKind::ReferenceUnion),
                vec![S::Return(Some(local(span, array, any)))],
                Vec::new(),
            ),
            S::Init(
                out,
                Expr {
                    span,
                    ty: any,
                    kind: E::Array(Vec::new()),
                },
            ),
        ];
        // A stand-in element type per element kind: reads of that kind are
        // boxed with the kind's tag, whatever the element's own static type.
        let mut stand_ins = vec![
            number,
            Type::Boolean,
            Type::String,
            Type::Null,
            Type::Undefined,
            Type::Error,
            records::intern(Vec::new(), self.types)?,
            dictionaries::intern(Type::Unknown, self.types)?,
            any,
            unions::intern(vec![Type::String, Type::Undefined], self.types)?,
        ];
        if self.types.profile.bigint_values() {
            stand_ins.push(Type::BigInt);
        }
        for element in stand_ins {
            let Some(kind) = element.profile_kind(self.types.profile) else {
                continue;
            };
            let typed = arrays::intern(element, self.types)?;
            let view = || Expr {
                span,
                ty: typed,
                kind: E::Reinterpret(Box::new(local(span, array, any))),
            };
            let read = Expr {
                span,
                ty: element,
                kind: E::Index(Box::new(view()), Box::new(local(span, position, number))),
            };
            let boxed = unknown::widen(read, self.types)?;
            let copy = S::For(
                vec![S::Set(
                    position,
                    Expr {
                        span,
                        ty: number,
                        kind: if number == Type::Float64 {
                            E::Float64(0.0)
                        } else {
                            E::Integer(0)
                        },
                    },
                )],
                Some(Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Binary(
                        ast::BinaryOp::Lt,
                        Box::new(local(span, position, number)),
                        Box::new(Expr {
                            span,
                            ty: number,
                            kind: E::Length(Box::new(view())),
                        }),
                    ),
                }),
                vec![S::Set(
                    position,
                    Expr {
                        span,
                        ty: number,
                        kind: E::Binary(
                            ast::BinaryOp::Add,
                            Box::new(local(span, position, number)),
                            Box::new(Expr {
                                span,
                                ty: number,
                                kind: if number == Type::Float64 {
                                    E::Float64(1.0)
                                } else {
                                    E::Integer(1)
                                },
                            }),
                        ),
                    },
                )],
                vec![S::Expr(Expr {
                    span,
                    ty: number,
                    kind: E::ArrayPush(
                        Box::new(local(span, out, any)),
                        Box::new(Expr {
                            span,
                            ty: any,
                            kind: E::Array(vec![Some(boxed)]),
                        }),
                    ),
                })],
            );
            body.push(S::If(
                kind_test(kind),
                vec![copy, S::Return(Some(local(span, out, any)))],
                Vec::new(),
            ));
        }
        let error = self.array_error(
            ErrorClass::TypeError,
            "an array of function or promise values cannot be read as unknown[]",
            span,
        )?;
        body.push(S::Throw(error));
        Ok(body)
    }
}
