//! V38 array destructuring of iterables (`expression_forms`).
//!
//! `const [a, b] = source` and `[a, b] = source` where `source` is not an
//! array or tuple iterate it as `for...of` does (`Lower::iterated`):
//! strings by code point, collections, iterable class instances and
//! generators. A generator is stepped once per element the pattern names
//! (until it is done) and, when the pattern has no rest element and the
//! generator is not done, closed with `return()`, as ECMAScript
//! IteratorClose does; a rest element takes every remaining value. The
//! values taken are then destructured as an array.
//!
//! Deviation: the elements are taken before the pattern's defaults and
//! targets are evaluated, where JavaScript interleaves each step with its
//! target; the order is observable only when both have side effects.
use super::*;
use ir::GeneratorOutcome as Outcome;
use iteration::Iterated;
use varta_common::function::frame::Completion;

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
fn boolean(span: swc_common::Span, value: bool) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Integer(i32::from(value)),
    }
}
fn undefined(span: swc_common::Span) -> Expr {
    Expr {
        span,
        ty: Type::Undefined,
        kind: E::Integer(0),
    }
}

impl Lower<'_> {
    /// The array a destructuring pattern reads: `value` itself when it is an
    /// array or tuple (or may be `null`/`undefined`), otherwise the
    /// elements the pattern takes from the iterable `value`.
    pub(super) fn destructured_source(
        &mut self,
        value: Expr,
        pattern: &ast::ArrayPat,
    ) -> Result<Expr> {
        let arrays = unions::members(value.ty, self.types).iter().any(|ty| {
            matches!(
                ty,
                Type::Array(_) | Type::Tuple(_) | Type::Null | Type::Undefined
            )
        });
        if arrays || value.ty.is_union() {
            return Ok(value);
        }
        match self.iterated(value, "array destructuring")? {
            Iterated::Array { value, .. } => Ok(value),
            Iterated::Generator(generator) => self.taken(generator, pattern),
        }
    }

    /// The first elements of the generator `value` that `pattern` names.
    fn taken(&mut self, value: Expr, pattern: &ast::ArrayPat) -> Result<Expr> {
        let span = value.span;
        let ty = value.ty;
        let Type::Generator(id) = ty else {
            return Err(CompilerError::Internal(
                "missing destructured generator".into(),
            ));
        };
        let generator = self.types.generators[id as usize];
        let rest = pattern
            .elems
            .iter()
            .any(|entry| matches!(entry, Some(ast::Pat::Rest(_))));
        let wanted = pattern
            .elems
            .iter()
            .filter(|entry| !matches!(entry, Some(ast::Pat::Rest(_))))
            .count();
        if !rest && !generators::accepts_undefined(generator.returned, self.types) {
            return Err(self.source.locate(
                type_error(
                    "array destructuring of a generator without a rest element closes it, which requires TReturn to accept undefined",
                ),
                span,
            ));
        }
        let element = generator.yielded;
        let array = arrays::intern(element, self.types)?;
        let output = self.hidden(array);
        let iterator = self.hidden(ty);
        let outcome = self.hidden(Type::Number);
        let count = self.hidden(Type::Float64);
        let done = self.hidden(Type::Boolean);
        let it = || local(span, iterator, ty);
        let input = unions::widen(undefined(span), Some(generator.next), self.types)?;
        let mut step = Vec::new();
        if !rest {
            step.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Binary(
                        ast::BinaryOp::GtEq,
                        Box::new(local(span, count, Type::Float64)),
                        Box::new(number(span, wanted as f64)),
                    ),
                },
                vec![S::Break],
                vec![],
            ));
        }
        step.extend(self.generator_step(span, it(), Completion::Normal, input, outcome)?);
        step.push(S::If(
            generators::is(span, outcome, Outcome::Yielded),
            vec![],
            vec![S::Set(done, boolean(span, true)), S::Break],
        ));
        let yielded = Expr {
            span,
            ty: element,
            kind: E::GeneratorValue(Box::new(it())),
        };
        step.push(self.pushed(output, array, element, yielded)?);
        step.push(S::Set(
            count,
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Binary(
                    ast::BinaryOp::Add,
                    Box::new(local(span, count, Type::Float64)),
                    Box::new(number(span, 1.0)),
                ),
            },
        ));
        let mut body = vec![
            S::Init(
                output,
                Expr {
                    span,
                    ty: array,
                    kind: E::Array(vec![]),
                },
            ),
            S::Init(iterator, value),
            S::Init(count, number(span, 0.0)),
            S::Init(done, boolean(span, false)),
            S::While(boolean(span, true), step),
        ];
        if !rest {
            let closing = self.hidden(Type::Number);
            let input = if generator.returned == Type::Void {
                undefined(span)
            } else {
                unions::widen(undefined(span), Some(generator.returned), self.types)?
            };
            let close = self.generator_step(span, it(), Completion::Return, input, closing)?;
            body.push(S::If(local(span, done, Type::Boolean), vec![], close));
        }
        let marker = self.hidden(Type::Number);
        Ok(Expr {
            span,
            ty: array,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![output, iterator],
                body,
            },
        })
    }
}
