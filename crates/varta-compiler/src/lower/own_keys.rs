//! Dynamic string lookup over a bounded closed shape. Reuse exact UTF-16
//! comparison and presence reads without exposing payloads or prototype lookup.
//! The decision tree also serves typed record reads with prepared leaf bodies.
use super::*;
fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}
fn compare(span: swc_common::Span, key: u32, name: &[u8], op: ast::BinaryOp) -> Expr {
    Expr {
        span,
        ty: Type::Boolean,
        kind: E::Binary(
            op,
            Box::new(local(span, key, Type::String)),
            Box::new(Expr {
                span,
                ty: Type::String,
                kind: E::String(name.to_vec()),
            }),
        ),
    }
}
pub(super) fn lookup(span: swc_common::Span, key: u32, fields: &mut [(Vec<u8>, Vec<S>)]) -> Vec<S> {
    if fields.is_empty() {
        return vec![];
    }
    let middle = fields.len() / 2;
    let (left, rest) = fields.split_at_mut(middle);
    // The nonempty check above ensures middle is strictly below fields.len().
    let ((name, body), right) = rest.split_first_mut().expect("nonempty lookup partition");
    let miss = if left.is_empty() && right.is_empty() {
        vec![]
    } else {
        vec![S::If(
            compare(span, key, name, ast::BinaryOp::Lt),
            lookup(span, key, left),
            lookup(span, key, right),
        )]
    };
    vec![S::If(
        compare(span, key, name, ast::BinaryOp::EqEqEq),
        std::mem::take(body),
        miss,
    )]
}
impl Lower<'_> {
    pub(super) fn dynamic_error_own(
        &mut self,
        span: swc_common::Span,
        value: Expr,
        key: Expr,
    ) -> Expr {
        let ty = value.ty;
        let source = self.hidden(ty);
        let name = self.hidden(Type::String);
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let body = vec![
            S::Init(source, value),
            S::Init(name, key),
            S::Init(
                output,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(0),
                },
            ),
            S::If(
                compare(
                    span,
                    name,
                    &self.types.profile.string_bytes("message"),
                    ast::BinaryOp::EqEqEq,
                ),
                vec![S::Set(
                    output,
                    Expr {
                        span,
                        ty: Type::Boolean,
                        kind: E::ErrorOwn(Box::new(local(span, source, ty))),
                    },
                )],
                vec![],
            ),
        ];
        Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![source, name, output],
                body,
            },
        }
    }
    pub(super) fn dynamic_record_own(
        &mut self,
        span: swc_common::Span,
        value: Expr,
        key: Expr,
        shape: u32,
    ) -> Expr {
        let record = Type::Record(shape);
        let source = self.hidden(record);
        let name = self.hidden(Type::String);
        let output = self.hidden(Type::Boolean);
        let marker = self.hidden(Type::Number);
        let mut fields: Vec<_> = self.types.records[shape as usize]
            .fields
            .iter()
            .enumerate()
            .map(|(slot, field)| (slot as u32, field.name.as_str()))
            .collect();
        // Shape slots are sorted as UTF-8 metadata; JS string comparison uses
        // UTF-16 units. Reorder the decision tree, never the original slots.
        fields.sort_by(|a, b| a.1.encode_utf16().cmp(b.1.encode_utf16()));
        let mut fields: Vec<_> = fields
            .into_iter()
            .map(|(slot, name)| {
                (
                    self.types.profile.string_bytes(name),
                    vec![S::Set(
                        output,
                        Expr {
                            span,
                            ty: Type::Boolean,
                            kind: E::HasOwn(Box::new(local(span, source, record)), Some(slot)),
                        },
                    )],
                )
            })
            .collect();
        let mut body = vec![
            S::Init(source, value),
            S::Init(name, key),
            S::Init(
                output,
                Expr {
                    span,
                    ty: Type::Boolean,
                    kind: E::Integer(0),
                },
            ),
        ];
        body.extend(lookup(span, name, &mut fields));
        Expr {
            span,
            ty: Type::Boolean,
            kind: E::ValueBlock {
                output,
                marker,
                roots: vec![source, name, output],
                body,
            },
        }
    }
}
