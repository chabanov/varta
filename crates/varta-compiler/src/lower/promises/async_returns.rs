//! Typed async return completions retained until all finalizers finish.
use super::*;

/// Exit of a rewritten async body: a `return` stores its value and leaves
/// the labelled body, which then settles the async function's promise.
pub(super) struct Exit {
    pub(super) label: u32,
    pub(super) value: Option<u32>,
    pub(super) promise: u32,
    pub(super) adopt: u32,
    pub(super) promise_ty: Type,
    covariant: BTreeMap<Type, (usize, u32)>,
    selection: Option<u32>,
    reset: bool,
}
impl Exit {
    pub(super) fn new(
        lower: &mut Lower<'_>,
        label: u32,
        value: Type,
        promise_ty: Type,
        body: &[S],
    ) -> Self {
        let mut exit = Self {
            label,
            value: (value != Type::Void).then(|| lower.hidden(value)),
            promise: lower.hidden(promise_ty),
            adopt: lower.hidden(Type::Boolean),
            promise_ty,
            covariant: BTreeMap::new(),
            selection: None,
            reset: lower.types.profile.structural_subtyping(),
        };
        if exit.reset {
            let mut sources = std::collections::BTreeSet::new();
            returned_promises(body, &mut sources);
            sources.remove(&promise_ty);
            if !sources.is_empty() {
                exit.selection = Some(lower.hidden(Type::Float64));
                for (index, ty) in sources.into_iter().enumerate() {
                    exit.covariant.insert(ty, (index + 1, lower.hidden(ty)));
                }
            }
        }
        exit
    }
    pub(super) fn initialize(&self, span: swc_common::Span) -> Vec<S> {
        let mut body = Vec::new();
        if let Some(selection) = self.selection {
            body.push(S::Init(selection, float(span, 0.0)));
        }
        body.extend(
            self.covariant
                .iter()
                .map(|(ty, (_, slot))| S::Init(*slot, zero(span, *ty))),
        );
        body
    }
    pub(super) fn settlement(
        &self,
        lower: &mut Lower<'_>,
        span: swc_common::Span,
        target: Expr,
        fulfilled: Expr,
        (resolve, fulfill): (usize, usize),
    ) -> Result<S> {
        let mut settlement = S::If(
            local(span, self.adopt, Type::Boolean),
            vec![invoke(
                span,
                resolve,
                vec![target.clone(), local(span, self.promise, self.promise_ty)],
            )],
            vec![invoke(span, fulfill, vec![target.clone(), fulfilled])],
        );
        let Type::Promise(target_id) = self.promise_ty else {
            return Err(invalid("missing async target promise"));
        };
        for (ty, (index, slot)) in &self.covariant {
            let Type::Promise(source_id) = *ty else {
                return Err(invalid("missing async source promise"));
            };
            let selection = self
                .selection
                .ok_or_else(|| invalid("missing async promise selection"))?;
            let resolve = lower.resolve_typed_helper(span, target_id, source_id)?;
            settlement = S::If(
                compare(
                    span,
                    ast::BinaryOp::EqEqEq,
                    local(span, selection, Type::Float64),
                    float(span, *index as f64),
                ),
                vec![invoke(
                    span,
                    resolve,
                    vec![target.clone(), local(span, *slot, *ty)],
                )],
                vec![settlement],
            );
        }
        Ok(settlement)
    }
}

fn returned_promises(body: &[S], sources: &mut std::collections::BTreeSet<Type>) {
    for statement in body {
        match statement {
            S::Return(Some(value)) if matches!(value.ty, Type::Promise(_)) => {
                sources.insert(value.ty);
            }
            S::Block(body)
            | S::Scope(_, body)
            | S::Label { body, .. }
            | S::While(_, body)
            | S::DoWhile(_, body) => returned_promises(body, sources),
            S::If(_, yes, no) => {
                returned_promises(yes, sources);
                returned_promises(no, sources);
            }
            S::TryCatch { body, handler, .. } => {
                returned_promises(body, sources);
                returned_promises(handler, sources);
            }
            S::TryFinally {
                body, finalizer, ..
            } => {
                returned_promises(body, sources);
                returned_promises(finalizer, sources);
            }
            S::For(init, _, _, body) => {
                returned_promises(init, sources);
                returned_promises(body, sources);
            }
            S::Switch { cases, .. } => {
                for (_, body) in cases {
                    returned_promises(body, sources);
                }
            }
            _ => {}
        }
    }
}
pub(super) fn rewrite(body: Vec<S>, exit: &Exit) -> Vec<S> {
    body.into_iter()
        .map(|s| rewrite_statement(s, exit))
        .collect()
}
fn rewrite_statement(statement: S, exit: &Exit) -> S {
    match statement {
        S::Return(None) if !exit.reset => S::BreakLabel(exit.label),
        S::Return(None) => {
            let span = swc_common::DUMMY_SP;
            let mut body = vec![S::Set(exit.adopt, boolean(span, false))];
            if let Some(selection) = exit.selection {
                body.push(S::Set(selection, float(span, 0.0)));
            }
            body.push(S::BreakLabel(exit.label));
            S::Block(body)
        }
        S::Return(Some(value)) => {
            let span = value.span;
            let source = exit.covariant.get(&value.ty);
            let own = value.ty == exit.promise_ty;
            let mut exit_body = if value.ty == exit.promise_ty {
                vec![
                    S::Set(exit.promise, value),
                    S::Set(exit.adopt, boolean(span, true)),
                ]
            } else if let Some((_, slot)) = source {
                vec![S::Set(*slot, value)]
            } else if let Some(slot) = exit.value {
                vec![S::Set(slot, value)]
            } else {
                vec![S::Expr(value)]
            };
            // A finalizer can replace any earlier completion, including a
            // promise return with a value or a different promise type.
            if exit.reset && !own {
                exit_body.push(S::Set(exit.adopt, boolean(span, false)));
            }
            if let Some(selection) = exit.selection {
                let selected = source.map_or(0, |(index, _)| *index);
                exit_body.push(S::Set(selection, float(span, selected as f64)));
            }
            exit_body.push(S::BreakLabel(exit.label));
            S::Block(exit_body)
        }
        S::TryCatch {
            body,
            binding,
            handler,
            marker,
        } => S::TryCatch {
            body: rewrite(body, exit),
            binding,
            handler: rewrite(handler, exit),
            marker,
        },
        S::TryFinally {
            body,
            finalizer,
            marker,
            completion,
            exception,
            result,
        } => S::TryFinally {
            body: rewrite(body, exit),
            finalizer: rewrite(finalizer, exit),
            marker,
            completion,
            exception,
            result,
        },
        S::Block(body) => S::Block(rewrite(body, exit)),
        S::Scope(slots, body) => S::Scope(slots, rewrite(body, exit)),
        S::Label {
            id,
            iteration,
            body,
        } => S::Label {
            id,
            iteration,
            body: rewrite(body, exit),
        },
        S::If(test, yes, no) => S::If(test, rewrite(yes, exit), rewrite(no, exit)),
        S::While(test, body) => S::While(test, rewrite(body, exit)),
        S::DoWhile(test, body) => S::DoWhile(test, rewrite(body, exit)),
        S::For(init, test, update, body) => S::For(
            rewrite(init, exit),
            test,
            rewrite(update, exit),
            rewrite(body, exit),
        ),
        S::Switch {
            value,
            discriminant,
            selected,
            cases,
        } => S::Switch {
            value,
            discriminant,
            selected,
            cases: cases
                .into_iter()
                .map(|(test, body)| (test, rewrite(body, exit)))
                .collect(),
        },
        other => other,
    }
}
