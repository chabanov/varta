//! `for (const key in value)` over dictionaries and closed records (v35
//! `mixed_index_records`). The loop visits the own enumerable string keys in
//! ECMAScript order: it snapshots `Object.keys(value)` before the first
//! iteration, so keys added by the body are not visited (as V8 does), and
//! skips a snapshotted key that the body deleted before it was visited (as the
//! specification requires). Records and dictionaries have no enumerable
//! inherited keys, so nothing reaches `Object.prototype`.
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
    pub(super) fn for_in(&mut self, statement: &ast::ForInStmt) -> Result<S> {
        self.scopes.push(BTreeMap::new());
        let (pattern, mode) = match &statement.left {
            ast::ForHead::VarDecl(declaration)
                if declaration.kind != ast::VarDeclKind::Var
                    && declaration.decls.len() == 1
                    && declaration.decls[0].init.is_none() =>
            {
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
            _ => {
                return Err(self.source.locate(
                    unsupported("for-in var declaration, initializer or multiple bindings"),
                    statement.left.span(),
                ));
            }
        };
        if !matches!(pattern, ast::Pat::Ident(_)) {
            return Err(self
                .source
                .locate(unsupported("for-in destructuring head"), pattern.span()));
        }
        let value = self.object_value(&statement.right)?;
        if self.types.profile.refinement_writes() {
            self.flow_for_in_effects(statement);
        }
        let span = statement.span;
        let source_ty = value.ty;
        let shape = match source_ty {
            Type::Dictionary(_) => None,
            Type::Record(shape) if self.types.records[shape as usize].class.is_none() => {
                Some(shape)
            }
            _ => {
                return Err(self.source.locate(
                    type_error(format!(
                        "for-in requires a dictionary or closed record, found {source_ty}; \
                         iterate arrays with for-of"
                    )),
                    value.span,
                ));
            }
        };
        let entry = self.flow_snapshot();
        let target = self.flow_iteration();
        let source = self.hidden(source_ty);
        let keys = self.enumerate_value(
            local(span, source, source_ty),
            object_enumeration::Kind::Keys,
            span,
        )?;
        let keys_ty = keys.ty;
        let snapshot = self.hidden(keys_ty);
        let index = self.hidden(Type::Float64);
        let current = self.hidden(Type::String);
        let roots = vec![source, snapshot, index];
        let init = vec![
            S::Init(source, value),
            S::Init(snapshot, keys),
            S::Init(index, number(span, 0.0)),
        ];
        let key = local(span, current, Type::String);
        // A key the body deleted before its visit is skipped.
        let present = match shape {
            None => self.dictionary_has(local(span, source, source_ty), key, false)?,
            Some(shape) => {
                self.dynamic_record_own(span, local(span, source, source_ty), key, shape)
            }
        };
        let mut visit = self.binding_pattern(
            pattern,
            local(pattern.span(), current, Type::String),
            None,
            None,
            mode,
        )?;
        self.loops += 1;
        visit.extend(self.single(&statement.body)?);
        self.loops -= 1;
        if self.types.profile.refinement_writes() {
            self.flow_take(target, true);
            let mut paths = self.flow_end(target);
            paths.push(entry.clone()); // An empty input can skip every body store.
            self.flow_join(&entry, &paths)?;
        }
        let slots = self
            .scopes
            .pop()
            .ok_or_else(|| invalid("missing for-in scope"))?
            .values()
            .map(|b| b.slot)
            .collect::<Vec<_>>();
        let mut iteration = vec![current];
        iteration.extend(slots);
        let read = Expr {
            span,
            ty: Type::String,
            kind: E::Index(
                Box::new(local(span, snapshot, keys_ty)),
                Box::new(local(span, index, Type::Float64)),
            ),
        };
        let body = vec![S::Init(current, read), S::If(present, visit, vec![])];
        let test = binary(
            span,
            ast::BinaryOp::Lt,
            local(span, index, Type::Float64),
            Expr {
                span,
                ty: Type::Float64,
                kind: E::Length(Box::new(local(span, snapshot, keys_ty))),
            },
            Type::Boolean,
        );
        let update = S::Set(
            index,
            binary(
                span,
                ast::BinaryOp::Add,
                local(span, index, Type::Float64),
                number(span, 1.0),
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
}
