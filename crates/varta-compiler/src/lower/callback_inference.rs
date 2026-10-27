//! Infer unannotated callback block returns after one semantic lowering pass.
//! Function bodies live separately, so nested arrows cannot contribute returns.
//! Assignment and internal value blocks contain no source return/finally sites;
//! their value type is resolved and nested user functions live separately.
use super::*;

fn visit(body: &mut [S], visitor: &mut impl FnMut(&mut S) -> Result<()>) -> Result<()> {
    for statement in body {
        match statement {
            S::Block(body)
            | S::Scope(_, body)
            | S::While(_, body)
            | S::DoWhile(_, body)
            | S::Label { body, .. } => {
                visit(body, visitor)?;
            }
            S::If(_, yes, no) => {
                visit(yes, visitor)?;
                visit(no, visitor)?;
            }
            S::For(init, _, update, body) => {
                visit(init, visitor)?;
                visit(update, visitor)?;
                visit(body, visitor)?;
            }
            S::Switch { cases, .. } => {
                for (_, body) in cases {
                    visit(body, visitor)?;
                }
            }
            S::TryCatch { body, handler, .. } => {
                visit(body, visitor)?;
                visit(handler, visitor)?;
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                visit(body, visitor)?;
                visit(finalizer, visitor)?;
            }
            _ => {}
        }
        visitor(statement)?;
    }
    Ok(())
}

impl Lower<'_> {
    pub(super) fn infer_callback_returns(
        &mut self,
        body: &mut [S],
        span: swc_common::Span,
    ) -> Result<()> {
        let mut values = Vec::new();
        let mut bare = false;
        visit(body, &mut |statement| {
            if let S::Return(value) = statement {
                if let Some(value) = value {
                    values.push((value.ty, value.span));
                } else {
                    bare = true;
                }
            }
            Ok(())
        })?;
        // No value-returning site keeps the established void callback contract,
        // including callbacks that only throw or return without a value.
        let Some(&(mut result, _)) = values.first() else {
            self.result = Type::Void;
            self.infer_returns = false;
            return Ok(());
        };
        if bare || !returns(body) {
            values.push((Type::Undefined, span));
        }
        for (ty, at) in values {
            if ty != result {
                result = unions::intern(vec![result, ty], self.types)
                    .map_err(|error| self.source.locate(error, at))?;
            }
        }
        self.result = result;
        self.infer_returns = false;
        visit(body, &mut |statement| {
            if let S::Return(value) = statement {
                let value = value.take().unwrap_or(Expr {
                    span,
                    ty: Type::Undefined,
                    kind: E::Integer(0),
                });
                *statement = S::Return(Some(unions::widen(value, Some(result), self.types)?));
            }
            // Finally lowering deferred the physical saved-return type. Allocate
            // it now, including both slots of a result pair and its exact root.
            if let S::TryFinally { result: saved, .. } = statement {
                let slot = self.hidden(result);
                *saved = Some(slot);
                let inner = std::mem::replace(statement, S::Block(vec![]));
                *statement = S::Scope(vec![slot], vec![inner]);
            }
            Ok(())
        })
    }
}
