//! Compound, logical and update assignments on dictionary entries (v35
//! `mixed_index_records`): `d[k] += v`, `d[k] ??= v`, `d.n++`. The receiver
//! and key are evaluated once, the entry is read before the right-hand side
//! runs (as `undefined` when absent, or through the checked declared read of a
//! mixed record), and the result is stored as a dictionary write would store
//! it, typed by the declared field or the index type.
use super::*;

fn local(span: swc_common::Span, slot: u32, ty: Type) -> Expr {
    Expr {
        span,
        ty,
        kind: E::Local(slot),
    }
}

/// The operation of a dictionary entry update.
pub(super) enum Update<'a> {
    /// `d[k] op= rhs` for an arithmetic or bitwise operator.
    Compound(ast::BinaryOp, &'a ast::Expr),
    /// `d[k] &&= rhs`, `||=` or `??=`.
    Logical(ast::AssignOp, &'a ast::Expr),
    /// `d[k]++` / `--d[k]`.
    Step(&'a ast::UpdateExpr),
}

impl Lower<'_> {
    pub(super) fn dictionary_update(
        &mut self,
        member: &ast::MemberExpr,
        base: Expr,
        update: Update<'_>,
        span: swc_common::Span,
    ) -> Result<Expr> {
        let declared = mixed_records::declared_field(base.ty, member, self.types);
        if declared.is_none() {
            mixed_records::object_key(base.ty, member, self.types)
                .map_err(|e| self.source.locate(e, member.prop.span()))?;
        }
        let key = self.dictionary_key(member)?;
        let key = self.dictionary_key_value(key)?;
        let Type::Dictionary(id) = base.ty else {
            return Err(invalid("missing dictionary type"));
        };
        let ty = base.ty;
        let expected = declared
            .as_ref()
            .map_or(self.types.dictionaries[id as usize].value, |field| field.ty);
        let source = self.hidden(ty);
        let name = self.hidden(Type::String);
        let receiver = || local(span, source, ty);
        let entry = || local(span, name, Type::String);
        let old = match &declared {
            Some(field) => self.declared_get(member.span, receiver(), entry(), field)?,
            None => self.dictionary_get(receiver(), entry())?,
        };
        let mut body = vec![S::Init(source, base), S::Init(name, key)];
        let mut roots = vec![source, name];
        let value = match update {
            Update::Compound(op, rhs) => {
                let computed = self.compound_operation(old, rhs, op, span)?;
                let computed = self.checked_assignment_result(computed, expected)?;
                self.dictionary_set(receiver(), entry(), computed, expected)?
            }
            Update::Logical(op, rhs) => {
                let saved = self.flow_snapshot();
                let value = self.expr_expected(rhs, Some(expected))?;
                self.flow_restore(&saved);
                let store = self.dictionary_set(receiver(), entry(), value, expected)?;
                self.logical_assignment_operation(old, store, op, span)?
            }
            Update::Step(step) => {
                let old = self.checked_update_numeric(old)?;
                let input_ty = old.ty;
                let input = self.hidden(input_ty);
                roots.push(input);
                body.push(S::Init(input, old));
                let next = self.update_next(span, input, input_ty, step.op)?;
                let next = self.checked_assignment_result(next, expected)?;
                let stored = self.dictionary_set(receiver(), entry(), next, expected)?;
                if step.prefix {
                    stored
                } else {
                    // The postfix value is the old numeric value; the store
                    // still runs first.
                    Expr {
                        span,
                        ty: input_ty,
                        kind: E::Sequence(Box::new(stored), Box::new(local(span, input, input_ty))),
                    }
                }
            }
        };
        let result = value.ty;
        let output = self.hidden(result);
        let marker = self.hidden(Type::Number);
        roots.push(output);
        body.push(S::Init(output, value));
        Ok(Expr {
            span,
            ty: result,
            kind: E::ValueBlock {
                output,
                marker,
                roots,
                body,
            },
        })
    }
}
