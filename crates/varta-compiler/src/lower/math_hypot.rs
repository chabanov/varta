//! Cache all inputs, then use scaled compensated squares without host math.
use super::*;
use swc_common::Span;

fn number(span: Span, value: f64) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Float64(value),
    }
}
fn local(span: Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn binary(span: Span, op: ast::BinaryOp, left: Expr, right: Expr) -> Expr {
    let ty = if matches!(op, ast::BinaryOp::EqEqEq | ast::BinaryOp::Gt) {
        Type::Boolean
    } else {
        Type::Float64
    };
    Expr {
        span,
        ty,
        kind: E::Binary(op, Box::new(left), Box::new(right)),
    }
}
fn math(span: Span, mode: ir::MathIntrinsic, value: Expr) -> Expr {
    Expr {
        span,
        ty: Type::Float64,
        kind: E::Math(mode, vec![value]),
    }
}

const SIGNIFICAND_SPLITTER: f64 = 134217729.0; // 2^27+1, Dekker split
struct Square {
    split: u32,
    high: u32,
    low: u32,
    value: u32,
    error: u32,
}
impl Square {
    fn new(lower: &mut Lower<'_>) -> Self {
        Self {
            split: lower.hidden(Type::Float64),
            high: lower.hidden(Type::Float64),
            low: lower.hidden(Type::Float64),
            value: lower.hidden(Type::Float64),
            error: lower.hidden(Type::Float64),
        }
    }
    fn slots(&self) -> [u32; 5] {
        [self.split, self.high, self.low, self.value, self.error]
    }
    fn emit(&self, span: Span, input: u32) -> Vec<S> {
        let read = |slot| local(span, slot, Type::Float64);
        let op = |kind, left, right| binary(span, kind, left, right);
        use ast::BinaryOp::{Add, Mul, Sub};
        vec![
            S::Init(
                self.split,
                op(Mul, number(span, SIGNIFICAND_SPLITTER), read(input)),
            ),
            S::Init(
                self.high,
                op(
                    Sub,
                    read(self.split),
                    op(Sub, read(self.split), read(input)),
                ),
            ),
            S::Init(self.low, op(Sub, read(input), read(self.high))),
            S::Init(self.value, op(Mul, read(input), read(input))),
            S::Init(
                self.error,
                op(
                    Add,
                    op(
                        Add,
                        op(
                            Sub,
                            op(Mul, read(self.high), read(self.high)),
                            read(self.value),
                        ),
                        op(
                            Mul,
                            op(Mul, number(span, 2.0), read(self.high)),
                            read(self.low),
                        ),
                    ),
                    op(Mul, read(self.low), read(self.low)),
                ),
            ),
        ]
    }
}

impl Lower<'_> {
    pub(super) fn math_hypot(&mut self, call: &ast::CallExpr) -> Result<Expr> {
        let span = call.span;
        let mut inputs = Vec::new();
        let mut body = Vec::new();
        for arg in &call.args {
            if arg.spread.is_some() {
                return Err(unsupported("spread to a Math intrinsic"));
            }
            let value = self.expr(&arg.expr)?;
            if value.ty != Type::Float64 {
                return Err(self.source.locate(type_error("Math intrinsic argument must be a narrowed Number; coercion is unsupported"),value.span));
            }
            let slot = self.hidden(Type::Float64);
            inputs.push(slot);
            // Number abs is pure. Every input finishes before exceptional-value
            // selection; neither Infinity nor NaN skips a later argument.
            body.push(S::Init(slot, math(span, ir::MathIntrinsic::Abs, value)));
        }
        if inputs.is_empty() {
            return Ok(number(span, 0.0));
        }
        let maximum = self.hidden(Type::Float64);
        let nan = self.hidden(Type::Boolean);
        let sum = self.hidden(Type::Float64);
        let correction = self.hidden(Type::Float64);
        let ratio = self.hidden(Type::Float64);
        let adjusted = self.hidden(Type::Float64);
        let total = self.hidden(Type::Float64);
        let output = self.hidden(Type::Float64);
        let scale = self.hidden(Type::Float64);
        let root = self.hidden(Type::Float64);
        let square = Square::new(self);
        let marker = self.hidden(Type::Number);
        let read = |slot| local(span, slot, Type::Float64);
        body.push(S::Init(maximum, number(span, 0.0)));
        body.push(S::Init(
            nan,
            Expr {
                span,
                ty: Type::Boolean,
                kind: E::Integer(0),
            },
        ));
        for &input in &inputs {
            body.push(S::If(
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::NumberPredicate(ir::NumberPredicate::Nan, vec![read(input)]),
                },
                vec![S::Init(
                    nan,
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::Integer(1),
                    },
                )],
                vec![S::If(
                    binary(span, ast::BinaryOp::Gt, read(input), read(maximum)),
                    vec![S::Init(maximum, read(input))],
                    vec![],
                )],
            ));
        }
        let mut finite = vec![
            S::Init(
                scale,
                math(span, ir::MathIntrinsic::BinadeScale, read(maximum)),
            ),
            S::Init(sum, number(span, 0.0)),
            S::Init(correction, number(span, 0.0)),
        ];
        for &input in &inputs {
            finite.push(S::Init(
                ratio,
                binary(span, ast::BinaryOp::Div, read(input), read(scale)),
            ));
            finite.extend(square.emit(span, ratio));
            finite.extend([
                S::Init(
                    adjusted,
                    binary(
                        span,
                        ast::BinaryOp::Sub,
                        read(square.value),
                        read(correction),
                    ),
                ),
                S::Init(
                    total,
                    binary(span, ast::BinaryOp::Add, read(sum), read(adjusted)),
                ),
                S::Init(
                    correction,
                    binary(
                        span,
                        ast::BinaryOp::Sub,
                        binary(
                            span,
                            ast::BinaryOp::Sub,
                            binary(span, ast::BinaryOp::Sub, read(total), read(sum)),
                            read(adjusted),
                        ),
                        read(square.error),
                    ),
                ),
                S::Init(sum, read(total)),
            ]);
        }
        // Account for the product rounding in the square root residual too.
        // The final power-of-two rescaling avoids max*sqrt(sum) double rounding.
        finite.push(S::Init(
            root,
            math(span, ir::MathIntrinsic::Sqrt, read(sum)),
        ));
        finite.extend(square.emit(span, root));
        finite.push(S::Init(
            adjusted,
            binary(
                span,
                ast::BinaryOp::Sub,
                binary(
                    span,
                    ast::BinaryOp::Sub,
                    binary(span, ast::BinaryOp::Sub, read(sum), read(square.value)),
                    read(correction),
                ),
                read(square.error),
            ),
        ));
        finite.push(S::Init(
            root,
            binary(
                span,
                ast::BinaryOp::Add,
                read(root),
                binary(
                    span,
                    ast::BinaryOp::Div,
                    read(adjusted),
                    binary(span, ast::BinaryOp::Add, read(root), read(root)),
                ),
            ),
        ));
        finite.push(S::Init(
            output,
            binary(span, ast::BinaryOp::Mul, read(scale), read(root)),
        ));
        body.push(S::If(
            binary(
                span,
                ast::BinaryOp::EqEqEq,
                read(maximum),
                number(span, f64::INFINITY),
            ),
            vec![S::Init(output, number(span, f64::INFINITY))],
            vec![S::If(
                local(span, nan, Type::Boolean),
                vec![S::Init(output, number(span, f64::NAN))],
                vec![S::If(
                    binary(
                        span,
                        ast::BinaryOp::EqEqEq,
                        read(maximum),
                        number(span, 0.0),
                    ),
                    vec![S::Init(output, number(span, 0.0))],
                    finite,
                )],
            )],
        ));
        inputs.extend([
            maximum, nan, sum, correction, ratio, adjusted, total, output, scale, root,
        ]);
        inputs.extend(square.slots());
        Ok(Expr {
            span,
            ty: Type::Float64,
            kind: E::ValueBlock {
                output,
                marker,
                roots: inputs,
                body,
            },
        })
    }
}
